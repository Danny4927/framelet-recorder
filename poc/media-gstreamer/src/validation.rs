#![forbid(unsafe_code)]

//! Hardware-free reference models for the isolated media pipeline evaluation.
//!
//! These tests do not initialize GStreamer and do not exercise its queues,
//! bus, encoders, muxer, or native error handling. They validate only the
//! small invariants explicitly represented by these models.

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
    sample_position: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineError(&'static str);

#[derive(Default)]
struct StreamTimeline {
    end: Option<Duration>,
    sequence: Option<u64>,
    sample_position_end: Option<u64>,
}

fn validate_timeline(samples: &[TimedSample]) -> Result<(), TimelineError> {
    let mut audio = StreamTimeline::default();
    let mut video = StreamTimeline::default();

    for sample in samples {
        if sample.duration.is_zero() {
            return Err(TimelineError("sample duration must be non-zero"));
        }
        let end = sample
            .pts
            .checked_add(sample.duration)
            .ok_or(TimelineError("sample end timestamp overflow"))?;
        let state = match sample.stream {
            StreamKind::Audio => &mut audio,
            StreamKind::Video => &mut video,
        };

        if state.end.is_some_and(|previous| sample.pts < previous) {
            return Err(TimelineError("stream timestamps regress or overlap"));
        }
        if state.sequence.is_some_and(|previous| sample.sequence != previous + 1) {
            return Err(TimelineError("stream sequence is not contiguous"));
        }
        state.sequence = Some(sample.sequence);

        if sample.stream == StreamKind::Audio {
            let position = sample.sample_position.ok_or(TimelineError(
                "audio sample position must be provided",
            ))?;
            if state
                .sample_position_end
                .is_some_and(|previous| position != previous)
            {
                return Err(TimelineError("audio sample positions are not contiguous"));
            }
            let sample_count = u64::try_from(
                sample.duration.as_nanos().saturating_mul(48_000) / 1_000_000_000,
            )
            .map_err(|_| TimelineError("audio sample count overflow"))?;
            state.sample_position_end = Some(
                position
                    .checked_add(sample_count)
                    .ok_or(TimelineError("audio sample position overflow"))?,
            );
        } else if sample.sample_position.is_some() {
            return Err(TimelineError("video sample must not have an audio position"));
        }
        state.end = Some(end);
    }
    Ok(())
}

#[derive(Debug)]
struct BoundedMediaQueueModel<T> {
    capacity: usize,
    items: Vec<T>,
    rejected: u64,
}

impl<T> BoundedMediaQueueModel<T> {
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

    /// Single-threaded model: reject newest when full; never overwrite accepted items.
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

/// Reference model only; the running PoC uses GStreamer's pipeline-wide bus EOS.
#[derive(Debug)]
struct FinalizationBarrierModel {
    audio_terminal: Option<Terminal>,
    video_terminal: Option<Terminal>,
}

impl FinalizationBarrierModel {
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
            (Some(Terminal::Error), _) | (_, Some(Terminal::Error)) => Some(Finalization::Failed),
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

    fn video(pts_ms: u64, duration_ms: u64, sequence: u64) -> TimedSample {
        TimedSample {
            stream: StreamKind::Video,
            pts: Duration::from_millis(pts_ms),
            duration: Duration::from_millis(duration_ms),
            sequence,
            sample_position: None,
        }
    }

    fn audio(pts_ms: u64, duration_ms: u64, sequence: u64, sample_position: u64) -> TimedSample {
        TimedSample {
            stream: StreamKind::Audio,
            pts: Duration::from_millis(pts_ms),
            duration: Duration::from_millis(duration_ms),
            sequence,
            sample_position: Some(sample_position),
        }
    }

    #[test]
    fn accepts_interleaved_audio_and_video_timeline() {
        let samples = [
            video(0, 33, 0),
            audio(0, 10, 0, 0),
            audio(10, 10, 1, 480),
            audio(20, 10, 2, 960),
            video(33, 33, 1),
        ];
        assert_eq!(validate_timeline(&samples), Ok(()));
    }

    #[test]
    fn rejects_timestamp_regression_or_overlap_within_stream() {
        assert_eq!(
            validate_timeline(&[audio(20, 10, 0, 0), audio(10, 10, 1, 480)]),
            Err(TimelineError("stream timestamps regress or overlap"))
        );
    }

    #[test]
    fn rejects_zero_duration_and_overlapping_samples() {
        assert_eq!(
            validate_timeline(&[video(0, 0, 0)]),
            Err(TimelineError("sample duration must be non-zero"))
        );
        assert_eq!(
            validate_timeline(&[video(0, 40, 0), video(30, 40, 1)]),
            Err(TimelineError("stream timestamps regress or overlap"))
        );
    }

    #[test]
    fn rejects_sequence_gaps_and_duplicates() {
        assert_eq!(
            validate_timeline(&[video(0, 10, 0), video(10, 10, 2)]),
            Err(TimelineError("stream sequence is not contiguous"))
        );
        assert_eq!(
            validate_timeline(&[video(0, 10, 0), video(10, 10, 0)]),
            Err(TimelineError("stream sequence is not contiguous"))
        );
    }

    #[test]
    fn rejects_discontinuous_audio_sample_positions() {
        assert_eq!(
            validate_timeline(&[audio(0, 10, 0, 0), audio(10, 10, 1, 481)]),
            Err(TimelineError("audio sample positions are not contiguous"))
        );
    }

    #[test]
    fn queue_model_is_bounded_and_rejects_newest_without_overwriting() {
        let mut queue = BoundedMediaQueueModel::new(2).expect("non-zero capacity");
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
    fn queue_model_reject_counter_saturates_instead_of_wrapping() {
        let mut queue = BoundedMediaQueueModel::new(1).expect("non-zero capacity");
        queue.rejected = u64::MAX;
        assert_eq!(queue.try_push(1), Ok(()));
        assert_eq!(queue.try_push(2), Err(2));
        assert_eq!(queue.rejected(), u64::MAX);
    }

    #[test]
    fn rejects_zero_capacity_queue_model() {
        assert_eq!(
            BoundedMediaQueueModel::<u8>::new(0).err(),
            Some(TimelineError("queue capacity must be non-zero"))
        );
    }

    #[test]
    fn finalization_model_waits_for_both_streams_to_reach_eos() {
        let mut barrier = FinalizationBarrierModel::new();
        barrier.mark(StreamKind::Video, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), None);
        barrier.mark(StreamKind::Audio, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), Some(Finalization::Completed));
    }

    #[test]
    fn finalization_model_fails_when_either_stream_errors() {
        let mut barrier = FinalizationBarrierModel::new();
        barrier.mark(StreamKind::Video, Terminal::EndOfStream);
        barrier.mark(StreamKind::Audio, Terminal::Error);
        assert_eq!(barrier.outcome(), Some(Finalization::Failed));
    }

    #[test]
    fn finalization_model_treats_missing_terminal_stream_as_timeout() {
        let mut barrier = FinalizationBarrierModel::new();
        barrier.mark(StreamKind::Audio, Terminal::EndOfStream);
        assert_eq!(barrier.outcome(), None);
        assert_eq!(barrier.timeout(), Finalization::Failed);
    }
}
