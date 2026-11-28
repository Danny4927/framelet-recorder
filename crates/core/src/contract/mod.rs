//! Shared port contract suites and the fakes that pass them (roadmap A5).
//!
//! Compiled only for this crate's tests or with the `contract-tests` feature,
//! which adapter crates enable from `[dev-dependencies]` (ADR-0021). Each
//! suite is a plain `pub fn` that takes a factory for a fresh *subject* — the
//! adapter under test plus the test seams the suite needs — and panics with a
//! descriptive message on a contract violation. An adapter crate wires a
//! suite into its own `#[test]` functions, so the same assertions run against
//! the fakes here and against every real adapter.
//!
//! | Module | Port | Covers |
//! |---|---|---|
//! | [`storage`] | `RecordingStoragePort` | exclusive reservations, publish only the matching closed output, idempotent abort, typed errors |
//! | [`media`] | `MediaPipelinePort` + data plane | sinks match the configuration, lifecycle to the prepared target, idempotent abort, typed errors, video drops counted, audio discontinuity explicit (including trailing loss in the final stats), bounded queues |
//! | [`capture`] | screen/camera/microphone capture | delivery to the sink, idempotent stop releasing the sink, typed start failure, source loss reported through the sink |
//! | [`settings`] | `RecordingSettingsPort` | defaults when nothing is stored, exact round trip, defaults plus a typed error for unusable content that load leaves unchanged, failed save keeps the previous settings |

pub mod capture;
pub mod fakes;
pub mod media;
pub mod settings;
pub mod storage;

#[cfg(test)]
mod tests {
    //! The fakes pass every suite they are used in place of.

    use super::fakes::{
        FakeCaptureSubject, FakeMediaSubject, FakeSettingsSubject, FakeStorageSubject,
    };
    use super::{capture, media, settings, storage};

    #[test]
    fn fake_storage_reservations_are_exclusive() {
        storage::reservations_are_exclusive(FakeStorageSubject::default);
    }

    #[test]
    fn fake_storage_publishes_closed_output_once() {
        storage::publishes_closed_output_once(FakeStorageSubject::default);
    }

    #[test]
    fn fake_storage_abort_is_idempotent_and_spares_published_output() {
        storage::abort_is_idempotent_and_spares_published_output(FakeStorageSubject::default);
    }

    #[test]
    fn fake_storage_rejects_foreign_closed_output() {
        storage::rejects_foreign_closed_output(FakeStorageSubject::default);
    }

    #[test]
    fn fake_storage_rejects_unknown_session() {
        storage::rejects_unknown_session(FakeStorageSubject::default);
    }

    #[test]
    fn fake_media_prepares_sinks_for_exactly_the_required_sources() {
        media::prepares_sinks_for_exactly_the_required_sources(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_lifecycle_closes_to_the_prepared_target() {
        media::lifecycle_closes_to_the_prepared_target(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_abort_is_idempotent() {
        media::abort_is_idempotent(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_rejects_unknown_session() {
        media::rejects_unknown_session(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_video_overload_counts_drops() {
        media::video_overload_counts_drops(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_audio_overload_yields_discontinuity() {
        media::audio_overload_yields_discontinuity(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_audio_trailing_loss_is_reported_after_stream_end() {
        media::audio_trailing_loss_is_reported_after_stream_end(FakeMediaSubject::default);
    }

    #[test]
    fn fake_media_queues_never_exceed_capacity() {
        media::queues_never_exceed_capacity(FakeMediaSubject::default);
    }

    #[test]
    fn fake_screen_capture_passes_capture_contract() {
        capture::run_all(FakeCaptureSubject::screen);
    }

    #[test]
    fn fake_camera_capture_passes_capture_contract() {
        capture::run_all(FakeCaptureSubject::camera);
    }

    #[test]
    fn fake_microphone_capture_passes_capture_contract() {
        capture::run_all(FakeCaptureSubject::microphone);
    }

    #[test]
    fn fake_settings_missing_settings_load_as_defaults() {
        settings::missing_settings_load_as_defaults(FakeSettingsSubject::default);
    }

    #[test]
    fn fake_settings_saved_settings_round_trip() {
        settings::saved_settings_round_trip(FakeSettingsSubject::default);
    }

    #[test]
    fn fake_settings_unusable_settings_load_as_defaults_with_error() {
        settings::unusable_settings_load_as_defaults_with_error(FakeSettingsSubject::default);
    }

    #[test]
    fn fake_settings_failed_save_keeps_previous_settings() {
        settings::failed_save_keeps_previous_settings(FakeSettingsSubject::default);
    }

    #[test]
    fn run_all_entry_points_cover_every_suite() {
        storage::run_all(FakeStorageSubject::default);
        media::run_all(FakeMediaSubject::default);
        settings::run_all(FakeSettingsSubject::default);
    }
}
