#![forbid(unsafe_code)]

//! Deterministic, hardware-free tests for the isolated media pipeline evaluation.
//! These tests model queue and terminal-state invariants; they do not initialize
//! GStreamer or claim native plugin, playback, device, packaging, or performance validation.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimedSample {
    stream: StreamKind,
    pts: Duration,
    duration: Duration,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineError(&'static str);

fn validate_timeline(samples: &[TimedSample]) -> Result<(), TimelineError> {
    let mut last_audio_end = None;
    let mut last_video_end = None;

    for sample in samples {
        if sample.duration.is_zero() {
            return Err(TimelineError("sample duration must be non-zero"));
        }
        let end = sample
            .pts
            .checked_add(sample.duration)
            .ok_or(TimelineError("sample end timestamp overflow"))?;
        let previous_end = match sample.stream {
            StreamKind::Audio => &mut last_audio_end,
            StreamKind::Video => &mut last_video_end,
        };
        if previous_end.is_some_and(|previous| sample.pts < previous) {
            return Err(TimelineError("stream timestamps regress or overlap"));
        }
        *previous_end = Some(end);
    }
    Ok(())
}

#[derive(Debug)]
struct BoundedMediaQueue<T> {
    capacity: usize,
    items: Vec<T>,
    rejected: u64,
}

impl<T> BoundedMediaQueue<T> {
    fn new(capacity: usize) -> Result<Self, TimelineError> {
        if capacity == 0 {
            return Err(TimelineError("queue capacity must be non-zero"));
        }
        Ok(Self {
            capacity,
            items: Vec::with_capacity(capacity),
            rejected: 0,
        })
    }

    /// Reject newest media when full. Accepted items are never overwritten.
    fn try_push(&mut self, item: T) -> Result<(), T> {
        if self.items.len() == self.capacity {
            self.rejected = self.rejected.saturating_add(1);
            return Err(item);
        }
        self.items.push(item);
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn rejected(&self) -> u64 {
        self.rejected
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminal {
    EndOfStream,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finalization {
    Completed,
    Failed,
}

#[derive(Debug)]
struct FinalizationBarrier {
    audio_terminal: Option<Terminal>,
    video_terminal: Option<Terminal>,
}

impl FinalizationBarrier {
    fn new() -> Self {
        Self {
            audio_terminal: None,
            video_terminal: None,
        }
    }

    fn mark(&mut self, stream: StreamKind, terminal: Terminal) {
        match stream {
            StreamKind::Audio => self.audio_terminal = Some(terminal),
            StreamKind::Video => self.video_terminal = Some(terminal),
        }
    }

    fn outcome(&self) -> Option<Finalization> {
        match (self.audio_terminal, self.video_terminal) {
            (Some(Terminal::EndOfStream), Some(Terminal::EndOfStream)) => {
                Some(Finalization::Completed)
            }
            (Some(Terminal::Error), _) | (_, Some(Terminal::Error)) => {
                Some(Finalization::Failed)
            }
            _ => None,
        }
    }

    fn timeout(&self) -> Finalization {
        Finalization::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(stream: StreamKind, pts_ms: u64, duration_ms: u64, sequence: u64) -> TimedSample {
        TimedSample {
            stream,
            pts: Duration::from_millis(pts_ms),
            duration: Duration::from_millis(duration_ms),
            sequence,
        }
    }

    #[test]
    fn accepts_interleaved_audio_and_video_timeline() {
        let samples = [
            sample(StreamKind::Video, 0, 33, 0),
            sample(StreamKind::Audio, 0, 10, 0),
            sample(StreamKind::Audio, 10, 10, 1),
            sample(StreamKind::Audio, 20, 10, 2),
            sample(StreamKind::Video, 33, 33, 1),
        ];
        assert_eq!(validate_timeline(&samples), Ok(()));
    }

    #[test]
    fn rejects_timestamp_regression_within_stream() {
        let samples = [
            sample(StreamKind::Audio, 20, 10, 0),
            sample(StreamKind::Audio, 10, 10, 1),
        ];
        assert_eq!(
            validate_timeline(&samples),
            Err(TimelineError("stream timestamps regress or overlap"))
        );
    }

    #[test]
    fn rejects_zero_duration_and_overlapping_samples() {
        assert_eq!(
            validate_timeline(&[sample(StreamKind::Video, 0, 0, 0)]),
            Err(TimelineError("sample duration must be non-zero"))
        );
        assert_eq!(
            validate_timeline(&[
                sample(StreamKind::Video, 0, 40, 0),
                sample(StreamKind::Video, 30, 40, 1),
            ]),
            Err(TimelineError("stream timestamps regress or overlap"))
        );
    }

    #[test]
    fn queue_is_bounded_and_rejects_newest_without_overwriting() {
        let mut queue = BoundedMediaQueue::new(2).expect("non-zero capacity");
        assert_eq!(queue.try_push(10), Ok(()));
        assert_eq!(queue.try_push(20), Ok(()));
        assert_eq!(queue.try_push(30), Err(30));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.rejected(), 1);
        assert_eq!(queue.pop(), Some(10));
        assert_eq!(queue.pop(), Some(20));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn queue_reject_counter_saturates_instead_of_wrapping() {
        let mut queue = BoundedMediaQueue::new(1).expect("non-zero capacity");
        queue.rejected = u64::MAX;
        assert_eq!(queue.try_push(1), Ok(()));
        assert_eq!(queue.try_push(2), Err(2));
        assert_eq!(queue.rejected(), u64::MAX);
    }

    #[test]
    fn rejects_zero_capacity_queue() {
        assert_eq!(
            BoundedMediaQueue::<u8>::new(0).err(),
            Some(TimelineError("queue capacity must be non-zero"))
        );
    }

    #[test]
    fn finalization_waits_for_both_streams_to_reach_eos() {
        let mut barrier = FinalizationBarrier::new();
        barrier.mark(StreamKind::Video, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), None);
        barrier.mark(StreamKind::Audio, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), Some(Finalization::Completed));
    }

    #[test]
    fn any_stream_error_prevents_successful_finalization() {
        let mut barrier = FinalizationBarrier::new();
        barrier.mark(StreamKind::Video, Terminal::EndOfStream);
        barrier.mark(StreamKind::Audio, Terminal::Error);
        assert_eq!(barrier.outcome(), Some(Finalization::Failed));
    }

    #[test]
    fn missing_terminal_stream_is_a_failed_timeout_not_success() {
        let mut barrier = FinalizationBarrier::new();
        barrier.mark(StreamKind::Audio, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), None);
        assert_eq!(barrier.timeout(), Finalization::Failed);
    }
}
