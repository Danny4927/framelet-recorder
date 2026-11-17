#![forbid(unsafe_code)]

//! Bounded, non-blocking hand-off between media producers and the pipeline worker.
//!
//! The channel has a fixed capacity. Producers use try_send semantics: when full,
//! the newest block is rejected and returned to the caller so audio loss can be
//! surfaced as a discontinuity instead of silently blocking a callback.

use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueMetricsSnapshot {
    pub capacity: usize,
    pub queued: usize,
    pub high_watermark: usize,
    pub accepted: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCapacity;

struct Metrics {
    capacity: usize,
    queued: AtomicUsize,
    high_watermark: AtomicUsize,
    accepted: AtomicU64,
    rejected: AtomicU64,
}

pub struct MediaProducer<T> {
    sender: SyncSender<T>,
    metrics: Arc<Metrics>,
}

impl<T> Clone for MediaProducer<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            metrics: Arc::clone(&self.metrics),
        }
    }
}

pub struct MediaConsumer<T> {
    receiver: Receiver<T>,
    metrics: Arc<Metrics>,
}

pub fn bounded<T>(capacity: usize) -> Result<(MediaProducer<T>, MediaConsumer<T>), InvalidCapacity> {
    if capacity == 0 {
        return Err(InvalidCapacity);
    }
    let (sender, receiver) = sync_channel(capacity);
    let metrics = Arc::new(Metrics {
        capacity,
        queued: AtomicUsize::new(0),
        high_watermark: AtomicUsize::new(0),
        accepted: AtomicU64::new(0),
        rejected: AtomicU64::new(0),
    });
    Ok((
        MediaProducer {
            sender,
            metrics: Arc::clone(&metrics),
        },
        MediaConsumer { receiver, metrics },
    ))
}

impl<T> MediaProducer<T> {
    /// Never waits for capacity. A full queue rejects the newest item.
    pub fn try_push(&self, item: T) -> Result<(), T> {
        let mut queued = self.metrics.queued.load(Ordering::Relaxed);
        loop {
            if queued >= self.metrics.capacity {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(item);
            }
            match self.metrics.queued.compare_exchange_weak(
                queued,
                queued + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => queued = actual,
            }
        }

        self.metrics.high_watermark.fetch_max(queued + 1, Ordering::Relaxed);
        match self.sender.try_send(item) {
            Ok(()) => {
                self.metrics.accepted.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(item) | TrySendError::Disconnected(item)) => {
                self.metrics.queued.fetch_sub(1, Ordering::AcqRel);
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                Err(item)
            }
        }
    }

    pub fn metrics(&self) -> QueueMetricsSnapshot {
        snapshot(&self.metrics)
    }
}

impl<T> MediaConsumer<T> {
    /// Blocks only the pipeline worker, never a producer or device callback.
    pub fn pop(&self) -> Option<T> {
        match self.receiver.recv() {
            Ok(item) => {
                self.metrics.queued.fetch_sub(1, Ordering::AcqRel);
                Some(item)
            }
            Err(_) => None,
        }
    }

    #[cfg(test)]
    pub fn try_pop(&self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(item) => {
                self.metrics.queued.fetch_sub(1, Ordering::AcqRel);
                Some(item)
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    #[cfg(test)]
    pub fn metrics(&self) -> QueueMetricsSnapshot {
        snapshot(&self.metrics)
    }
}

fn snapshot(metrics: &Metrics) -> QueueMetricsSnapshot {
    QueueMetricsSnapshot {
        capacity: metrics.capacity,
        queued: metrics.queued.load(Ordering::Acquire),
        high_watermark: metrics.high_watermark.load(Ordering::Relaxed),
        accepted: metrics.accepted.load(Ordering::Relaxed),
        rejected: metrics.rejected.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn rejects_zero_capacity() {
        assert!(bounded::<u8>(0).is_err());
    }

    #[test]
    fn enforces_capacity_and_counts_rejections_without_overwriting() {
        let (producer, consumer) = bounded(2).expect("positive capacity");
        assert_eq!(producer.try_push(10), Ok(()));
        assert_eq!(producer.try_push(20), Ok(()));
        assert_eq!(producer.try_push(30), Err(30));
        assert_eq!(consumer.try_pop(), Some(10));
        assert_eq!(consumer.try_pop(), Some(20));
        assert_eq!(consumer.try_pop(), None);

        let metrics = producer.metrics();
        assert_eq!(metrics.capacity, 2);
        assert_eq!(metrics.queued, 0);
        assert_eq!(metrics.high_watermark, 2);
        assert_eq!(metrics.accepted, 2);
        assert_eq!(metrics.rejected, 1);
    }

    #[test]
    fn supports_multiple_producers_and_a_consumer_without_exceeding_capacity() {
        let (producer, consumer) = bounded(8).expect("positive capacity");
        let first = producer.clone();
        let second = producer.clone();
        let a = thread::spawn(move || {
            for value in 0..100 {
                let _ = first.try_push(value);
            }
        });
        let b = thread::spawn(move || {
            for value in 100..200 {
                let _ = second.try_push(value);
            }
        });
        a.join().expect("first producer");
        b.join().expect("second producer");

        let metrics = consumer.metrics();
        assert!(metrics.queued <= metrics.capacity);
        assert!(metrics.high_watermark <= metrics.capacity);
        assert_eq!(metrics.accepted + metrics.rejected, 200);
        while consumer.try_pop().is_some() {}
        assert_eq!(consumer.metrics().queued, 0);
    }
}
