//! Data-plane unit tests: capacity, overload policy, gap marking, stream end.

use std::thread;
use std::time::Duration;

use framelet_capture_api::{AudioBlock, PixelFormat, VideoFrame, VideoFrameMetadata};

use super::*;
use crate::domain::RecordingMode;

fn frame(sequence: u64) -> VideoFrame {
    VideoFrame::new(
        VideoFrameMetadata {
            width: 1,
            height: 1,
            stride: 4,
            pixel_format: PixelFormat::Bgra8,
            timestamp: Duration::from_millis(sequence),
            sequence,
        },
        vec![0; 4],
    )
    .expect("valid frame")
}

fn block(frames: usize) -> AudioBlock {
    AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![0.0; frames * 2]).expect("valid block")
}

fn sequence_of(received: Received<VideoFrame>) -> Option<u64> {
    match received {
        Received::Item(frame) => Some(frame.metadata.sequence),
        _ => None,
    }
}

#[test]
fn capacity_is_validated_against_zero_and_maximum() {
    for (capacity, valid) in [
        (0, false),
        (1, true),
        (MAX_VIDEO_QUEUE_CAPACITY, true),
        (MAX_VIDEO_QUEUE_CAPACITY + 1, false),
    ] {
        assert_eq!(video_channel(capacity).is_ok(), valid, "video {capacity}");
    }
    for (capacity, valid) in [
        (0, false),
        (MAX_AUDIO_QUEUE_CAPACITY, true),
        (MAX_AUDIO_QUEUE_CAPACITY + 1, false),
    ] {
        assert_eq!(audio_channel(capacity).is_ok(), valid, "audio {capacity}");
    }
    assert_eq!(
        video_channel(0).err(),
        Some(DataPlaneError::InvalidCapacity {
            requested: 0,
            maximum: MAX_VIDEO_QUEUE_CAPACITY
        })
    );
}

#[test]
fn video_overload_drops_oldest_counts_and_never_exceeds_capacity() {
    let (mut sink, mut stream) = video_channel(3).expect("channel");
    let mut outcomes = Vec::new();
    for sequence in 0..7 {
        outcomes.push(sink.try_send(frame(sequence)));
        assert!(stream.stats().queued <= 3);
    }
    assert_eq!(&outcomes[..3], &[VideoDelivery::Accepted; 3]);
    assert_eq!(
        &outcomes[3..],
        &[1, 2, 3, 4].map(|dropped_total| VideoDelivery::DroppedStale { dropped_total })
    );
    let stats = stream.stats();
    assert_eq!(
        (
            stats.accepted,
            stats.dropped,
            stats.queued,
            stats.high_water
        ),
        (7, 4, 3, 3)
    );
    // The newest frames survive; the stale ones were dropped.
    let survivors: Vec<_> = (0..3)
        .filter_map(|_| sequence_of(stream.try_recv()))
        .collect();
    assert_eq!(survivors, [4, 5, 6]);
    assert_eq!(stream.try_recv(), Received::Empty);
    assert_eq!(stream.stats().delivered, 3);
}

#[test]
fn audio_overload_rejects_newest_and_marks_one_gap_per_episode() {
    let (mut sink, mut stream) = audio_channel(2).expect("channel");
    assert_eq!(sink.try_send(block(10)), AudioDelivery::Accepted);
    assert_eq!(sink.try_send(block(10)), AudioDelivery::Accepted);
    assert_eq!(
        sink.try_send(block(5)),
        AudioDelivery::Discontinuity {
            lost_blocks_total: 1
        }
    );
    assert_eq!(
        sink.try_send(block(7)),
        AudioDelivery::Discontinuity {
            lost_blocks_total: 2
        }
    );
    let stats = stream.stats();
    assert_eq!(
        (
            stats.queued,
            stats.dropped,
            stats.lost_audio_frames,
            stats.discontinuities
        ),
        (2, 2, 12, 1)
    );

    // Accepted audio is retained unmarked; the next accepted block carries the gap.
    for _ in 0..2 {
        match stream.try_recv() {
            Received::Item(packet) => assert_eq!(packet.gap_before, None),
            other => panic!("expected packet, got {other:?}"),
        }
    }
    assert_eq!(sink.try_send(block(3)), AudioDelivery::Accepted);
    match stream.try_recv() {
        Received::Item(packet) => assert_eq!(
            packet.gap_before,
            Some(AudioGap {
                lost_blocks: 2,
                lost_frames: 12
            })
        ),
        other => panic!("expected marked packet, got {other:?}"),
    }

    // A second overload is a second episode.
    for _ in 0..3 {
        let _ = sink.try_send(block(1));
    }
    assert_eq!(stream.stats().discontinuities, 2);
}

#[test]
fn trailing_audio_loss_is_reported_in_final_stats_after_stream_end() {
    let (mut sink, mut stream) = audio_channel(1).expect("channel");
    assert_eq!(sink.try_send(block(4)), AudioDelivery::Accepted);
    assert!(matches!(
        sink.try_send(block(6)),
        AudioDelivery::Discontinuity { .. }
    ));
    assert_eq!(
        stream.stats().pending_gap,
        Some(AudioGap {
            lost_blocks: 1,
            lost_frames: 6
        })
    );
    drop(sink);
    match stream.try_recv() {
        Received::Item(packet) => assert_eq!(packet.gap_before, None),
        other => panic!("expected the accepted packet, got {other:?}"),
    }
    assert_eq!(stream.try_recv(), Received::Ended(StreamEnd::Stopped));
    // No packet carries the loss, so the final stats must.
    let stats = stream.stats();
    assert_eq!(
        stats.pending_gap,
        Some(AudioGap {
            lost_blocks: 1,
            lost_frames: 6
        })
    );
    assert_eq!((stats.dropped, stats.discontinuities), (1, 1));
}

#[test]
fn marked_gap_is_no_longer_pending() {
    let (mut sink, mut stream) = audio_channel(1).expect("channel");
    let _ = sink.try_send(block(1));
    let _ = sink.try_send(block(1));
    let _ = stream.try_recv();
    assert_eq!(sink.try_send(block(1)), AudioDelivery::Accepted);
    assert_eq!(stream.stats().pending_gap, None);
}

#[test]
fn dropping_the_sink_ends_the_stream_after_queued_items() {
    let (mut sink, mut stream) = video_channel(2).expect("channel");
    assert_eq!(sink.try_send(frame(1)), VideoDelivery::Accepted);
    drop(sink);
    assert_eq!(sequence_of(stream.try_recv()), Some(1));
    assert_eq!(stream.try_recv(), Received::Ended(StreamEnd::Stopped));
}

#[test]
fn reported_source_loss_is_visible_to_consumer_and_monitor() {
    let (sink, mut stream) = audio_channel(2).expect("channel");
    let monitor = sink.monitor();
    assert!(!monitor.source_lost());
    sink.report_source_lost();
    assert!(monitor.source_lost());
    assert_eq!(stream.try_recv(), Received::Ended(StreamEnd::SourceLost));
}

#[test]
fn closed_consumer_rejects_sends_and_releases_queue() {
    let (mut video, video_stream) = video_channel(2).expect("channel");
    let monitor = video.monitor();
    let _ = video.try_send(frame(1));
    drop(video_stream);
    assert_eq!(video.try_send(frame(2)), VideoDelivery::Closed);
    assert_eq!(monitor.stats().queued, 0);
    assert!(monitor.stats().consumer_closed);

    let (mut audio, audio_stream) = audio_channel(2).expect("channel");
    drop(audio_stream);
    assert_eq!(audio.try_send(block(1)), AudioDelivery::Closed);
}

#[test]
fn recv_timeout_wakes_on_item_and_on_end() {
    let (mut sink, mut stream) = video_channel(1).expect("channel");
    assert_eq!(
        stream.recv_timeout(Duration::from_millis(1)),
        Received::Empty
    );
    let producer = thread::spawn(move || {
        let _ = sink.try_send(frame(9));
    });
    assert_eq!(
        sequence_of(stream.recv_timeout(Duration::from_secs(10))),
        Some(9)
    );
    producer.join().expect("producer");
    assert_eq!(
        stream.recv_timeout(Duration::from_secs(10)),
        Received::Ended(StreamEnd::Stopped)
    );
}

#[test]
fn concurrent_producer_never_exceeds_capacity() {
    let (mut sink, mut stream) = video_channel(4).expect("channel");
    let monitor = sink.monitor();
    let producer = thread::spawn(move || {
        for sequence in 0..2_000 {
            let _ = sink.try_send(frame(sequence));
        }
    });
    let mut last = None;
    loop {
        match stream.recv_timeout(Duration::from_secs(10)) {
            Received::Item(frame) => {
                // Order is preserved even when frames are dropped.
                assert!(last < Some(frame.metadata.sequence));
                last = Some(frame.metadata.sequence);
            }
            Received::Empty => {}
            Received::Ended(end) => {
                assert_eq!(end, StreamEnd::Stopped);
                break;
            }
        }
    }
    producer.join().expect("producer");
    let stats = monitor.stats();
    assert!(stats.high_water <= 4);
    assert_eq!(stats.accepted, 2_000);
    assert_eq!(stats.delivered + stats.dropped, 2_000);
}

#[test]
fn media_inputs_match_exactly_the_required_sources() {
    let capacities = StreamCapacities { video: 2, audio: 4 };
    for (mode, microphone) in [
        (RecordingMode::Screen, false),
        (RecordingMode::Screen, true),
        (RecordingMode::Microphone, false),
        (RecordingMode::Webcam, true),
        (RecordingMode::ScreenAndWebcam, true),
    ] {
        let configuration = RecordingConfiguration::new(mode, microphone).expect("configuration");
        let (sinks, streams) = media_inputs(&configuration, capacities).expect("inputs");
        assert!(sinks.matches(&configuration), "{mode:?} {microphone}");
        for kind in [
            SourceKind::Screen,
            SourceKind::Camera,
            SourceKind::Microphone,
        ] {
            assert_eq!(sinks.has(kind), configuration.requires_source(kind));
        }
        assert_eq!(
            streams.microphone.is_some(),
            configuration.requires_source(SourceKind::Microphone)
        );
        let other = RecordingConfiguration::new(RecordingMode::Webcam, false).expect("other");
        assert_eq!(sinks.matches(&other), configuration == other);
    }
    let configuration = RecordingConfiguration::new(RecordingMode::Screen, false).expect("config");
    assert!(media_inputs(&configuration, StreamCapacities { video: 0, audio: 1 }).is_err());
}

#[test]
fn taking_sinks_removes_them() {
    let configuration =
        RecordingConfiguration::new(RecordingMode::ScreenAndWebcam, true).expect("configuration");
    let (mut sinks, _streams) =
        media_inputs(&configuration, StreamCapacities { video: 1, audio: 1 }).expect("inputs");
    assert!(sinks.take_video(SourceKind::Microphone).is_none());
    assert!(sinks.take_video(SourceKind::Screen).is_some());
    assert!(sinks.take_video(SourceKind::Screen).is_none());
    assert!(sinks.take_audio().is_some());
    assert!(!sinks.has(SourceKind::Microphone));
    assert!(sinks.has(SourceKind::Camera));
}

#[test]
fn recv_timeout_returns_a_queued_item_without_waiting() {
    let (mut sink, mut stream) = video_channel(1).expect("channel");
    assert_eq!(sink.try_send(frame(3)), VideoDelivery::Accepted);
    let started = std::time::Instant::now();
    assert_eq!(
        sequence_of(stream.recv_timeout(Duration::from_secs(30))),
        Some(3)
    );
    // Waiting out the timeout would mean a queued item was not seen.
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn dropping_a_video_sink_is_visible_to_its_monitor() {
    let (sink, _stream) = video_channel(1).expect("channel");
    let monitor = sink.monitor();
    assert_eq!(monitor.stats().end, None);
    drop(sink);
    assert_eq!(monitor.stats().end, Some(StreamEnd::Stopped));
}

#[test]
fn data_plane_errors_and_debug_output_are_descriptive() {
    assert_eq!(
        DataPlaneError::InvalidCapacity {
            requested: 0,
            maximum: 8
        }
        .to_string(),
        "media queue capacity 0 is outside 1..=8"
    );
    let (video, video_stream) = video_channel(1).expect("channel");
    let (audio, audio_stream) = audio_channel(1).expect("channel");
    let monitor = video.monitor();
    for (text, prefix) in [
        (format!("{video:?}"), "VideoSink("),
        (format!("{video_stream:?}"), "VideoStream("),
        (format!("{audio:?}"), "AudioSink("),
        (format!("{audio_stream:?}"), "AudioStream("),
        (format!("{monitor:?}"), "StreamMonitor("),
    ] {
        assert!(text.starts_with(prefix), "{text}");
        assert!(text.contains("capacity: 1"), "{text}");
    }
}
