#![forbid(unsafe_code)]
//! Recording timeline: maps monotonic source timestamps onto presentation time.
//!
//! The [`RecordingTimeline`] aggregate owns the recording origin T0, the pause
//! segments, and the last timestamp seen per stream. Presentation time is the
//! active (non-paused) time elapsed since T0. Wall-clock time is never used.
//!
//! Pause segments are half-open intervals `[start, end)`: a timestamp equal to
//! a segment's start is paused, a timestamp equal to its end is presented.

use std::fmt;

use crate::domain::SourceKind;

/// Maximum number of closed pause segments one recording may contain.
pub const MAX_PAUSE_SEGMENTS: usize = 4096;

/// A monotonic-clock reading in nanoseconds. Never wall-clock time.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicInstant(u64);

impl MonotonicInstant {
    /// Creates an instant from monotonic nanoseconds.
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }
    /// Returns the instant in monotonic nanoseconds.
    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

/// Presentation timestamp (PTS): active nanoseconds since T0, pauses removed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresentationTimestamp(u64);

impl PresentationTimestamp {
    /// Creates a presentation timestamp from nanoseconds.
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }
    /// Returns the presentation timestamp in nanoseconds.
    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

/// A closed pause interval `[start, end)` excluded from presentation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PauseSegment {
    start: MonotonicInstant,
    end: MonotonicInstant,
}

impl PauseSegment {
    /// Creates a segment; `end` must not precede `start`.
    ///
    /// # Errors
    /// Returns [`TimelineError::ResumeBeforePause`] when `end < start`.
    pub const fn new(
        start: MonotonicInstant,
        end: MonotonicInstant,
    ) -> Result<Self, TimelineError> {
        if end.0 < start.0 {
            return Err(TimelineError::ResumeBeforePause);
        }
        Ok(Self { start, end })
    }
    /// Instant the pause began (inclusive).
    #[must_use]
    pub const fn start(self) -> MonotonicInstant {
        self.start
    }
    /// Instant the recording resumed (exclusive).
    #[must_use]
    pub const fn end(self) -> MonotonicInstant {
        self.end
    }
    /// Paused duration in nanoseconds; cannot overflow because `end >= start`.
    #[must_use]
    pub const fn duration_nanos(self) -> u64 {
        self.end.0.saturating_sub(self.start.0)
    }
}

/// Outcome of mapping one source timestamp onto the recording timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineMapping {
    /// The sample is presented at this timestamp.
    Presented(PresentationTimestamp),
    /// The sample falls inside a pause segment and is not presented.
    DuringPause,
    /// The sample precedes T0 and is not presented.
    BeforeOrigin,
    /// The source timestamp did not advance past the stream's previous one.
    Regression {
        /// Previous accepted source timestamp of the stream.
        previous: MonotonicInstant,
        /// Offending source timestamp.
        current: MonotonicInstant,
    },
    /// Active time since the stream's previous sample exceeds the threshold.
    /// The sample is still presented at `pts`; the gap must be surfaced.
    Discontinuity {
        /// Active (pause-excluded) gap in nanoseconds.
        gap: u64,
        /// Presentation timestamp of this sample.
        pts: PresentationTimestamp,
    },
}

/// Typed timeline misuse and arithmetic errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineError {
    /// A pause was requested while another pause is open.
    AlreadyPaused,
    /// A resume was requested with no open pause.
    NotPaused,
    /// A pause would begin before T0.
    PauseBeforeOrigin,
    /// A pause would begin before the previous pause segment ended, or at or
    /// before an already mapped sample (pauses never cover mapped samples
    /// retroactively, so per-stream PTS stay monotonic).
    PauseOutOfOrder,
    /// A resume instant precedes its pause start.
    ResumeBeforePause,
    /// A resume instant is at or before an already mapped sample.
    ResumeBeforeMappedSample,
    /// The closed pause segment limit [`MAX_PAUSE_SEGMENTS`] was reached.
    TooManyPauses,
    /// Checked timestamp arithmetic overflowed. Defensive: the aggregate's
    /// invariants keep every result within `u64`, so this signals an
    /// invariant violation rather than an expected runtime condition.
    Overflow,
}

impl fmt::Display for TimelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::AlreadyPaused => "timeline is already paused",
            Self::NotPaused => "timeline is not paused",
            Self::PauseBeforeOrigin => "pause begins before the timeline origin",
            Self::PauseOutOfOrder => "pause begins before the previous pause ended",
            Self::ResumeBeforePause => "resume precedes pause start",
            Self::ResumeBeforeMappedSample => "resume is not after the latest mapped sample",
            Self::TooManyPauses => "too many pause segments",
            Self::Overflow => "timestamp arithmetic overflowed",
        };
        f.write_str(text)
    }
}

impl std::error::Error for TimelineError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamCursor {
    source: MonotonicInstant,
    active: u64,
}

/// Aggregate root mapping all stream timestamps onto one recording timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingTimeline {
    origin: MonotonicInstant,
    discontinuity_threshold_nanos: u64,
    pauses: Vec<PauseSegment>,
    open_pause: Option<MonotonicInstant>,
    cursors: [Option<StreamCursor>; 3],
}

const fn stream_index(stream: SourceKind) -> usize {
    match stream {
        SourceKind::Screen => 0,
        SourceKind::Camera => 1,
        SourceKind::Microphone => 2,
    }
}

impl RecordingTimeline {
    /// Starts a timeline at origin T0 with a discontinuity threshold in ns.
    #[must_use]
    pub const fn new(origin: MonotonicInstant, discontinuity_threshold_nanos: u64) -> Self {
        Self {
            origin,
            discontinuity_threshold_nanos,
            pauses: Vec::new(),
            open_pause: None,
            cursors: [None; 3],
        }
    }

    /// Recording origin T0.
    #[must_use]
    pub const fn origin(&self) -> MonotonicInstant {
        self.origin
    }

    /// Closed pause segments in chronological order.
    #[must_use]
    pub fn pause_segments(&self) -> &[PauseSegment] {
        &self.pauses
    }

    /// Start of the open pause, if the timeline is paused.
    #[must_use]
    pub const fn open_pause(&self) -> Option<MonotonicInstant> {
        self.open_pause
    }

    /// Latest source timestamp mapped (and recorded) on any stream.
    fn latest_mapped_sample(&self) -> Option<MonotonicInstant> {
        self.cursors.iter().flatten().map(|c| c.source).max()
    }

    /// Opens a pause segment at `at`.
    ///
    /// # Errors
    /// Returns a [`TimelineError`] when already paused, when `at` precedes T0
    /// or the previous segment's end, when `at` is not after every sample
    /// already mapped, or when the segment cap is reached.
    pub fn begin_pause(&mut self, at: MonotonicInstant) -> Result<(), TimelineError> {
        if self.open_pause.is_some() {
            return Err(TimelineError::AlreadyPaused);
        }
        if at < self.origin {
            return Err(TimelineError::PauseBeforeOrigin);
        }
        let latest_sample = self.latest_mapped_sample();
        if self.pauses.last().is_some_and(|last| at < last.end)
            || latest_sample.is_some_and(|latest| at <= latest)
        {
            return Err(TimelineError::PauseOutOfOrder);
        }
        if self.pauses.len() >= MAX_PAUSE_SEGMENTS {
            return Err(TimelineError::TooManyPauses);
        }
        self.open_pause = Some(at);
        Ok(())
    }

    /// Closes the open pause segment at `at`; zero-length pauses are allowed.
    ///
    /// # Errors
    /// Returns [`TimelineError::NotPaused`] without an open pause and
    /// [`TimelineError::ResumeBeforePause`] when `at` precedes its start, and
    /// [`TimelineError::ResumeBeforeMappedSample`] when `at` is not after every
    /// sample already mapped (that sample was reported `DuringPause` and must
    /// not become active retroactively).
    pub fn end_pause(&mut self, at: MonotonicInstant) -> Result<(), TimelineError> {
        let start = self.open_pause.ok_or(TimelineError::NotPaused)?;
        if at < start {
            return Err(TimelineError::ResumeBeforePause);
        }
        if self
            .latest_mapped_sample()
            .is_some_and(|latest| at <= latest)
        {
            return Err(TimelineError::ResumeBeforeMappedSample);
        }
        let segment = PauseSegment::new(start, at)?;
        self.pauses.push(segment);
        self.open_pause = None;
        Ok(())
    }

    /// Active nanoseconds between T0 and `ts` (`ts >= origin`), plus whether
    /// `ts` lies inside a pause.
    fn active_elapsed(&self, ts: MonotonicInstant) -> Result<(u64, bool), TimelineError> {
        let elapsed =
            ts.0.checked_sub(self.origin.0)
                .ok_or(TimelineError::Overflow)?;
        let mut paused: u64 = 0;
        let mut inside = false;
        for segment in &self.pauses {
            if ts < segment.start {
                break;
            }
            let covered_end = if ts < segment.end { ts } else { segment.end };
            inside |= ts < segment.end;
            let covered = covered_end
                .0
                .checked_sub(segment.start.0)
                .ok_or(TimelineError::Overflow)?;
            paused = paused.checked_add(covered).ok_or(TimelineError::Overflow)?;
        }
        if let Some(start) = self.open_pause
            && ts >= start
        {
            let covered = ts.0.checked_sub(start.0).ok_or(TimelineError::Overflow)?;
            paused = paused.checked_add(covered).ok_or(TimelineError::Overflow)?;
            inside = true;
        }
        let active = elapsed.checked_sub(paused).ok_or(TimelineError::Overflow)?;
        Ok((active, inside))
    }

    /// Maps a source timestamp of `stream` onto the recording timeline.
    ///
    /// Samples before T0 and regressions do not advance the stream cursor;
    /// samples during a pause do.
    ///
    /// # Errors
    /// Returns [`TimelineError::Overflow`] if checked arithmetic fails; this is
    /// a defensive invariant-violation case that the invariants rule out.
    pub fn map(
        &mut self,
        stream: SourceKind,
        source_ts: MonotonicInstant,
    ) -> Result<TimelineMapping, TimelineError> {
        if source_ts < self.origin {
            return Ok(TimelineMapping::BeforeOrigin);
        }
        let index = stream_index(stream);
        let previous = self.cursors[index];
        if let Some(prev) = previous
            && source_ts <= prev.source
        {
            return Ok(TimelineMapping::Regression {
                previous: prev.source,
                current: source_ts,
            });
        }
        let (active, inside) = self.active_elapsed(source_ts)?;
        self.cursors[index] = Some(StreamCursor {
            source: source_ts,
            active,
        });
        if inside {
            return Ok(TimelineMapping::DuringPause);
        }
        let pts = PresentationTimestamp(active);
        if let Some(prev) = previous {
            let gap = active
                .checked_sub(prev.active)
                .ok_or(TimelineError::Overflow)?;
            if gap > self.discontinuity_threshold_nanos {
                return Ok(TimelineMapping::Discontinuity { gap, pts });
            }
        }
        Ok(TimelineMapping::Presented(pts))
    }
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
