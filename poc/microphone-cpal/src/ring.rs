#![forbid(unsafe_code)]
//! Fixed-capacity, single-producer/single-consumer audio sample ring buffer.
//!
//! The callback only copies into preallocated atomic slots and updates counters.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// A single-producer/single-consumer ring buffer of interleaved F32 samples.
pub struct AudioSampleRing {
    slots: Box<[AtomicU32]>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped_samples: AtomicU64,
}

/// Error returned when ring capacity is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RingBufferError {
    /// At least one sample slot is required.
    ZeroCapacity,
}

impl AudioSampleRing {
    /// Creates a ring with a fixed number of sample slots.
    ///
    /// This allocates before a capture stream is started.
    pub fn new(capacity_samples: usize) -> Result<Self, RingBufferError> {
        if capacity_samples == 0 {
            return Err(RingBufferError::ZeroCapacity);
        }
        let slots = (0..capacity_samples)
            .map(|_| AtomicU32::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(Self {
            slots,
            capacity: capacity_samples,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped_samples: AtomicU64::new(0),
        })
    }

    /// Returns the fixed capacity in individual samples.
    #[must_use]
    pub const fn capacity_samples(&self) -> usize {
        self.capacity
    }

    /// Returns the number of samples dropped because the ring was full.
    #[must_use]
    pub fn dropped_samples(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }

    /// Pushes callback samples without blocking or allocating.
    ///
    /// If capacity is exhausted, the remainder of the input slice is dropped
    /// and counted. This method assumes exactly one producer.
    pub fn push_from_callback(&self, input: &[f32]) -> usize {
        let mut written = 0;
        for sample in input {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);
            if head.wrapping_sub(tail) >= self.capacity {
                break;
            }
            let index = head % self.capacity;
            self.slots[index].store(sample.to_bits(), Ordering::Relaxed);
            self.head.store(head.wrapping_add(1), Ordering::Release);
            written += 1;
        }
        let dropped = input.len().saturating_sub(written);
        if dropped > 0 {
            self.dropped_samples.fetch_add(
                u64::try_from(dropped).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
        written
    }

    /// Copies up to the output slice length into a consumer-owned buffer.
    ///
    /// This method assumes exactly one consumer.
    pub fn pop_into(&self, output: &mut [f32]) -> usize {
        let mut read = 0;
        for slot in output {
            let tail = self.tail.load(Ordering::Relaxed);
            let head = self.head.load(Ordering::Acquire);
            if tail == head {
                break;
            }
            let index = tail % self.capacity;
            *slot = f32::from_bits(self.slots[index].load(Ordering::Relaxed));
            self.tail.store(tail.wrapping_add(1), Ordering::Release);
            read += 1;
        }
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_capacity() {
        assert_eq!(AudioSampleRing::new(0).err(), Some(RingBufferError::ZeroCapacity));
    }

    #[test]
    fn preserves_fifo_order() {
        let ring = AudioSampleRing::new(4).expect("test setup");
        assert_eq!(ring.push_from_callback(&[0.1, 0.2, 0.3]), 3);
        let mut output = [0.0; 3];
        assert_eq!(ring.pop_into(&mut output), 3);
        assert_eq!(output, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn counts_overflow_and_keeps_unread_samples() {
        let ring = AudioSampleRing::new(3).expect("test setup");
        assert_eq!(ring.push_from_callback(&[1.0, 2.0, 3.0, 4.0]), 3);
        assert_eq!(ring.dropped_samples(), 1);
        let mut output = [0.0; 3];
        assert_eq!(ring.pop_into(&mut output), 3);
        assert_eq!(output, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn wraps_around_without_changing_order() {
        let ring = AudioSampleRing::new(3).expect("test setup");
        ring.push_from_callback(&[1.0, 2.0]);
        let mut first = [0.0; 2];
        ring.pop_into(&mut first);
        ring.push_from_callback(&[3.0, 4.0, 5.0]);
        let mut second = [0.0; 3];
        assert_eq!(ring.pop_into(&mut second), 3);
        assert_eq!(second, [3.0, 4.0, 5.0]);
    }
}
