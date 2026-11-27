use super::*;

const STREAMS: [SourceKind; 3] = [
    SourceKind::Screen,
    SourceKind::Camera,
    SourceKind::Microphone,
];

fn at(nanos: u64) -> MonotonicInstant {
    MonotonicInstant::from_nanos(nanos)
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Begin(u64),
    End(u64),
    Map(usize, u64),
}

/// Independent u128 reference model of the timeline.
#[derive(Clone, Debug)]
struct Oracle {
    origin: u128,
    threshold: u128,
    segments: Vec<(u128, u128)>,
    open: Option<u128>,
    last: [Option<(u128, u128)>; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Expected {
    Ok(Option<TimelineMapping>),
    Err(TimelineError),
}

impl Oracle {
    fn new(origin: u64, threshold: u64) -> Self {
        Self {
            origin: u128::from(origin),
            threshold: u128::from(threshold),
            segments: Vec::new(),
            open: None,
            last: [None; 3],
        }
    }

    /// Active time computed by walking the complement of the pauses.
    fn active(&self, ts: u128) -> (u128, bool) {
        let mut cursor = self.origin;
        let mut active = 0u128;
        let mut inside = false;
        let pauses = self
            .segments
            .iter()
            .copied()
            .chain(self.open.map(|s| (s, u128::MAX)));
        for (start, end) in pauses {
            if start <= ts && ts < end {
                inside = true;
            }
            let run_end = start.min(ts);
            if run_end > cursor {
                active += run_end - cursor;
            }
            cursor = cursor.max(end);
        }
        if ts > cursor {
            active += ts - cursor;
        }
        (active, inside)
    }

    fn apply(&mut self, op: Op) -> Expected {
        match op {
            Op::Begin(t) => {
                let t = u128::from(t);
                if self.open.is_some() {
                    Expected::Err(TimelineError::AlreadyPaused)
                } else if t < self.origin {
                    Expected::Err(TimelineError::PauseBeforeOrigin)
                } else if self.segments.last().is_some_and(|&(_, end)| t < end)
                    || self.last.iter().flatten().any(|&(source, _)| t <= source)
                {
                    Expected::Err(TimelineError::PauseOutOfOrder)
                } else if self.segments.len() >= MAX_PAUSE_SEGMENTS {
                    Expected::Err(TimelineError::TooManyPauses)
                } else {
                    self.open = Some(t);
                    Expected::Ok(None)
                }
            }
            Op::End(t) => {
                let t = u128::from(t);
                match self.open {
                    None => Expected::Err(TimelineError::NotPaused),
                    Some(start) if t < start => Expected::Err(TimelineError::ResumeBeforePause),
                    Some(_) if self.last.iter().flatten().any(|&(source, _)| t <= source) => {
                        Expected::Err(TimelineError::ResumeBeforeMappedSample)
                    }
                    Some(start) => {
                        self.segments.push((start, t));
                        self.open = None;
                        Expected::Ok(None)
                    }
                }
            }
            Op::Map(stream, t) => {
                let ts = u128::from(t);
                if ts < self.origin {
                    return Expected::Ok(Some(TimelineMapping::BeforeOrigin));
                }
                let previous = self.last[stream];
                if let Some((prev, _)) = previous
                    && ts <= prev
                {
                    return Expected::Ok(Some(TimelineMapping::Regression {
                        previous: at(u64::try_from(prev).unwrap_or(u64::MAX)),
                        current: at(t),
                    }));
                }
                let (active, inside) = self.active(ts);
                let Ok(active64) = u64::try_from(active) else {
                    return Expected::Err(TimelineError::Overflow);
                };
                self.last[stream] = Some((ts, active));
                if inside {
                    return Expected::Ok(Some(TimelineMapping::DuringPause));
                }
                let pts = PresentationTimestamp::from_nanos(active64);
                if let Some((_, prev_active)) = previous {
                    let gap = active - prev_active;
                    if gap > self.threshold {
                        let Ok(gap) = u64::try_from(gap) else {
                            return Expected::Err(TimelineError::Overflow);
                        };
                        return Expected::Ok(Some(TimelineMapping::Discontinuity { gap, pts }));
                    }
                }
                Expected::Ok(Some(TimelineMapping::Presented(pts)))
            }
        }
    }
}

fn apply(timeline: &mut RecordingTimeline, op: Op) -> Expected {
    let result = match op {
        Op::Begin(t) => timeline.begin_pause(at(t)).map(|()| None),
        Op::End(t) => timeline.end_pause(at(t)).map(|()| None),
        Op::Map(stream, t) => timeline.map(STREAMS[stream], at(t)).map(Some),
    };
    match result {
        Ok(value) => Expected::Ok(value),
        Err(error) => Expected::Err(error),
    }
}

fn exhaust(
    timeline: &RecordingTimeline,
    oracle: &Oracle,
    alphabet: &[Op],
    depth: usize,
    trace: &mut Vec<Op>,
    count: &mut u64,
) {
    if depth == 0 {
        return;
    }
    for &op in alphabet {
        let mut timeline = timeline.clone();
        let mut oracle = oracle.clone();
        let actual = apply(&mut timeline, op);
        let expected = oracle.apply(op);
        trace.push(op);
        *count += 1;
        assert_eq!(
            actual,
            expected,
            "origin {} trace {trace:?}",
            timeline.origin().as_nanos()
        );
        // Ops that leave the state unchanged (errors, regressions, samples
        // before T0) lead to a subtree already covered by the shorter
        // sequence without that op, so only state-changing ops are expanded.
        let changed = matches!(
            expected,
            Expected::Ok(None)
                | Expected::Ok(Some(
                    TimelineMapping::Presented(_)
                        | TimelineMapping::DuringPause
                        | TimelineMapping::Discontinuity { .. }
                ))
        );
        if changed {
            exhaust(&timeline, &oracle, alphabet, depth - 1, trace, count);
        }
        trace.pop();
    }
}

fn alphabet(grid: &[u64], streams: usize) -> Vec<Op> {
    let mut ops = Vec::new();
    for &t in grid {
        ops.push(Op::Begin(t));
        ops.push(Op::End(t));
        for stream in 0..streams {
            ops.push(Op::Map(stream, t));
        }
    }
    ops
}

fn run_exhaustive(
    grid: &[u64],
    origins: &[u64],
    streams: usize,
    depth: usize,
    threshold: u64,
) -> u64 {
    let ops = alphabet(grid, streams);
    let mut count = 0;
    for &origin in origins {
        let timeline = RecordingTimeline::new(at(origin), threshold);
        let oracle = Oracle::new(origin, threshold);
        exhaust(&timeline, &oracle, &ops, depth, &mut Vec::new(), &mut count);
    }
    count
}

const LOW_GRID: [u64; 7] = [0, 1, 2, 3, 4, 5, 6];
const HIGH_GRID: [u64; 7] = [
    u64::MAX - 6,
    u64::MAX - 5,
    u64::MAX - 4,
    u64::MAX - 3,
    u64::MAX - 2,
    u64::MAX - 1,
    u64::MAX,
];

#[test]
fn exhaustive_single_stream_sequences_up_to_six_ops_match_oracle() {
    // Every origin and every op at every grid point.
    for grid in [LOW_GRID, HIGH_GRID] {
        let count = run_exhaustive(&grid, &grid, 1, 6, 1);
        assert!(count > 0);
    }
}

#[test]
fn exhaustive_multi_stream_sequences_up_to_five_ops_match_oracle() {
    for grid in [LOW_GRID, HIGH_GRID] {
        let count = run_exhaustive(&grid, &[grid[0], grid[3]], 3, 5, 1);
        assert!(count > 0);
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[test]
fn seeded_random_long_sequences_match_oracle() {
    const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rng = XorShift(SEED);
    for case in 0..10_000u32 {
        let high = rng.below(2) == 0;
        let base = if high { u64::MAX - 10_000 } else { 0 };
        let origin = base + rng.below(100);
        let threshold = rng.below(200);
        let mut timeline = RecordingTimeline::new(at(origin), threshold);
        let mut oracle = Oracle::new(origin, threshold);
        let mut now = origin;
        let length = 20 + rng.below(60);
        for step in 0..length {
            let jitter = rng.below(150);
            let t = match rng.below(8) {
                0 => base + rng.below(10_001),
                1 => now.saturating_sub(jitter),
                _ => now.saturating_add(jitter),
            };
            now = now.max(t);
            let op = match rng.below(6) {
                0 => Op::Begin(t),
                1 => Op::End(t),
                _ => Op::Map(usize::try_from(rng.below(3)).unwrap_or(0), t),
            };
            let actual = apply(&mut timeline, op);
            let expected = oracle.apply(op);
            assert_eq!(
                actual, expected,
                "seed {SEED:#x} case {case} step {step} op {op:?}"
            );
        }
    }
}

#[test]
fn maps_at_u64_max_without_overflow() {
    let mut timeline = RecordingTimeline::new(at(0), u64::MAX);
    assert_eq!(
        timeline.map(SourceKind::Screen, at(u64::MAX)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(u64::MAX)
        ))
    );
    let mut timeline = RecordingTimeline::new(at(u64::MAX), 0);
    assert_eq!(
        timeline.map(SourceKind::Screen, at(u64::MAX)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(0)
        ))
    );
    assert_eq!(
        timeline.map(SourceKind::Screen, at(u64::MAX - 1)),
        Ok(TimelineMapping::BeforeOrigin)
    );
}

#[test]
fn total_paused_equal_to_elapsed_presents_zero() {
    let mut timeline = RecordingTimeline::new(at(10), 1_000);
    assert_eq!(timeline.begin_pause(at(10)), Ok(()));
    assert_eq!(timeline.end_pause(at(u64::MAX)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Microphone, at(u64::MAX)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(0)
        ))
    );
}

#[test]
fn pause_edges_are_half_open_and_zero_length_pauses_are_ignored() {
    let mut timeline = RecordingTimeline::new(at(0), 1_000);
    assert_eq!(timeline.begin_pause(at(5)), Ok(()));
    assert_eq!(timeline.end_pause(at(5)), Ok(()));
    assert_eq!(timeline.begin_pause(at(10)), Ok(()));
    assert_eq!(timeline.end_pause(at(20)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Screen, at(5)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(5)
        ))
    );
    assert_eq!(
        timeline.map(SourceKind::Screen, at(10)),
        Ok(TimelineMapping::DuringPause)
    );
    assert_eq!(
        timeline.map(SourceKind::Screen, at(20)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(10)
        ))
    );
}

#[test]
fn open_pause_excludes_later_samples() {
    let mut timeline = RecordingTimeline::new(at(0), 1_000);
    assert_eq!(timeline.begin_pause(at(0)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Camera, at(0)),
        Ok(TimelineMapping::DuringPause)
    );
    assert_eq!(
        timeline.begin_pause(at(1)),
        Err(TimelineError::AlreadyPaused)
    );
    assert_eq!(timeline.end_pause(at(3)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Camera, at(4)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(1)
        ))
    );
}

#[test]
fn regression_and_duplicate_are_detected_without_advancing_cursor() {
    let mut timeline = RecordingTimeline::new(at(0), 1_000);
    assert!(matches!(
        timeline.map(SourceKind::Screen, at(10)),
        Ok(TimelineMapping::Presented(_))
    ));
    let duplicate = TimelineMapping::Regression {
        previous: at(10),
        current: at(10),
    };
    assert_eq!(timeline.map(SourceKind::Screen, at(10)), Ok(duplicate));
    let regression = TimelineMapping::Regression {
        previous: at(10),
        current: at(9),
    };
    assert_eq!(timeline.map(SourceKind::Screen, at(9)), Ok(regression));
    // Other streams are tracked independently.
    assert!(matches!(
        timeline.map(SourceKind::Microphone, at(9)),
        Ok(TimelineMapping::Presented(_))
    ));
}

#[test]
fn discontinuity_threshold_is_exclusive() {
    let mut timeline = RecordingTimeline::new(at(0), 100);
    assert!(matches!(
        timeline.map(SourceKind::Screen, at(0)),
        Ok(TimelineMapping::Presented(_))
    ));
    assert_eq!(
        timeline.map(SourceKind::Screen, at(100)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(100)
        ))
    );
    assert_eq!(
        timeline.map(SourceKind::Screen, at(201)),
        Ok(TimelineMapping::Discontinuity {
            gap: 101,
            pts: PresentationTimestamp::from_nanos(201)
        })
    );
}

#[test]
fn pause_gap_is_not_a_discontinuity() {
    let mut timeline = RecordingTimeline::new(at(0), 100);
    assert!(matches!(
        timeline.map(SourceKind::Screen, at(50)),
        Ok(TimelineMapping::Presented(_))
    ));
    assert_eq!(timeline.begin_pause(at(60)), Ok(()));
    assert_eq!(timeline.end_pause(at(10_000)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Screen, at(10_050)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(110)
        ))
    );
}

#[test]
fn pause_misuse_is_rejected_with_typed_errors() {
    let mut timeline = RecordingTimeline::new(at(10), 0);
    assert_eq!(timeline.end_pause(at(11)), Err(TimelineError::NotPaused));
    assert_eq!(
        timeline.begin_pause(at(9)),
        Err(TimelineError::PauseBeforeOrigin)
    );
    assert_eq!(timeline.begin_pause(at(12)), Ok(()));
    assert_eq!(
        timeline.end_pause(at(11)),
        Err(TimelineError::ResumeBeforePause)
    );
    assert_eq!(timeline.end_pause(at(15)), Ok(()));
    assert_eq!(
        timeline.begin_pause(at(14)),
        Err(TimelineError::PauseOutOfOrder)
    );
    assert_eq!(timeline.pause_segments().len(), 1);
}

#[test]
fn pause_segment_count_is_capped() {
    let mut timeline = RecordingTimeline::new(at(0), 0);
    for _ in 0..MAX_PAUSE_SEGMENTS {
        assert_eq!(timeline.begin_pause(at(1)), Ok(()));
        assert_eq!(timeline.end_pause(at(1)), Ok(()));
    }
    assert_eq!(
        timeline.begin_pause(at(1)),
        Err(TimelineError::TooManyPauses)
    );
    assert_eq!(timeline.open_pause(), None);
}

#[test]
fn resume_before_mapped_paused_sample_is_rejected() {
    let mut timeline = RecordingTimeline::new(at(0), 1_000);
    assert_eq!(timeline.begin_pause(at(50)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Camera, at(100)),
        Ok(TimelineMapping::DuringPause)
    );
    assert_eq!(
        timeline.end_pause(at(80)),
        Err(TimelineError::ResumeBeforeMappedSample)
    );
    assert_eq!(
        timeline.end_pause(at(100)),
        Err(TimelineError::ResumeBeforeMappedSample)
    );
    assert_eq!(timeline.open_pause(), Some(at(50)));
    assert_eq!(timeline.end_pause(at(101)), Ok(()));
    assert_eq!(
        timeline.map(SourceKind::Camera, at(102)),
        Ok(TimelineMapping::Presented(
            PresentationTimestamp::from_nanos(51)
        ))
    );
}

#[test]
fn timestamp_values_round_trip() {
    assert_eq!(at(7).as_nanos(), 7);
    assert_eq!(PresentationTimestamp::from_nanos(9).as_nanos(), 9);
    let segment = PauseSegment::new(at(10), at(25)).expect("ordered segment");
    assert_eq!(segment.duration_nanos(), 15);
}

#[test]
fn first_sample_exactly_at_a_closed_pause_start_is_inside_the_pause() {
    let mut timeline = RecordingTimeline::new(at(0), u64::MAX);
    assert!(matches!(
        timeline.map(SourceKind::Screen, at(5)),
        Ok(TimelineMapping::Presented(_))
    ));
    timeline.begin_pause(at(10)).expect("pause");
    timeline.end_pause(at(20)).expect("resume");
    assert_eq!(
        timeline.map(SourceKind::Camera, at(10)),
        Ok(TimelineMapping::DuringPause)
    );
}

#[test]
fn timeline_errors_have_descriptive_messages() {
    assert_eq!(
        TimelineError::ResumeBeforePause.to_string(),
        "resume precedes pause start"
    );
}
