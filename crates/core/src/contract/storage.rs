//! Contract suite for [`RecordingStoragePort`] (ADR-0021).
//!
//! Every suite takes a factory that returns a fresh [`StorageSubject`] and
//! panics with a descriptive message when the adapter breaks the contract.

use crate::domain::{RecordingArtifact, RecordingConfiguration, RecordingId, RecordingMode};
use crate::ports::{ClosedOutput, OutputTarget, PortError, RecordingStoragePort};

/// A storage adapter under test plus the means to stand in for a media writer.
pub trait StorageSubject {
    /// Returns the adapter under test.
    fn storage(&mut self) -> &mut dyn RecordingStoragePort;
    /// Writes `bytes` to a reserved target, as the media writer would.
    fn write_media(&mut self, target: &OutputTarget, bytes: &[u8]);
    /// Returns whether temporary (unpublished) output exists for `target`.
    fn candidate_exists(&self, target: &OutputTarget) -> bool;
    /// Returns the content published for `artifact`, if any.
    fn published_media(&self, artifact: &RecordingArtifact) -> Option<Vec<u8>>;
}

fn configuration() -> RecordingConfiguration {
    match RecordingConfiguration::new(RecordingMode::Screen, false) {
        Ok(configuration) => configuration,
        Err(error) => panic!("valid configuration rejected: {error}"),
    }
}

fn is_invalid_input<T: std::fmt::Debug>(result: &Result<T, PortError>) -> bool {
    matches!(result, Err(PortError::InvalidInput(_)))
}

/// Two reservations get distinct sessions and distinct targets, and each
/// target exists as temporary output.
pub fn reservations_are_exclusive<S: StorageSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let first = subject
        .storage()
        .begin(RecordingId::new(1), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    let second = subject
        .storage()
        .begin(RecordingId::new(1), &configuration())
        .unwrap_or_else(|error| panic!("second begin failed: {error}"));
    assert_ne!(first.session, second.session, "sessions must be distinct");
    assert_ne!(first.target, second.target, "targets must be distinct");
    assert!(subject.candidate_exists(&first.target));
    assert!(subject.candidate_exists(&second.target));
    for reservation in [first, second] {
        assert_eq!(subject.storage().abort(&reservation.session), Ok(()));
    }
}

/// begin → write → close → publish exposes exactly the written media and
/// consumes the reservation; publishing the same session again fails.
pub fn publishes_closed_output_once<S: StorageSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let reservation = subject
        .storage()
        .begin(RecordingId::new(2), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    subject.write_media(&reservation.target, b"complete media");
    let artifact = subject
        .storage()
        .publish(
            &reservation.session,
            ClosedOutput::new(reservation.target.clone()),
        )
        .unwrap_or_else(|error| panic!("publish failed: {error}"));
    assert_eq!(
        subject.published_media(&artifact).as_deref(),
        Some(&b"complete media"[..]),
        "published content must be the closed output"
    );
    assert!(
        !subject.candidate_exists(&reservation.target),
        "temporary output must be gone after publication"
    );
    let again = subject.storage().publish(
        &reservation.session,
        ClosedOutput::new(reservation.target.clone()),
    );
    assert!(is_invalid_input(&again), "second publish: {again:?}");
    assert_eq!(
        subject.published_media(&artifact).as_deref(),
        Some(&b"complete media"[..]),
        "a rejected publish must not touch the published recording"
    );
}

/// Abort removes the temporary output, can be repeated, and never touches a
/// published recording. A publish after abort fails.
pub fn abort_is_idempotent_and_spares_published_output<S: StorageSubject>(
    mut make: impl FnMut() -> S,
) {
    let mut subject = make();
    let aborted = subject
        .storage()
        .begin(RecordingId::new(3), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    subject.write_media(&aborted.target, b"partial");
    assert_eq!(subject.storage().abort(&aborted.session), Ok(()));
    assert!(!subject.candidate_exists(&aborted.target));
    assert_eq!(
        subject.storage().abort(&aborted.session),
        Ok(()),
        "abort must be idempotent"
    );
    let late = subject
        .storage()
        .publish(&aborted.session, ClosedOutput::new(aborted.target.clone()));
    assert!(is_invalid_input(&late), "publish after abort: {late:?}");

    let published = subject
        .storage()
        .begin(RecordingId::new(4), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    subject.write_media(&published.target, b"kept");
    let artifact = subject
        .storage()
        .publish(
            &published.session,
            ClosedOutput::new(published.target.clone()),
        )
        .unwrap_or_else(|error| panic!("publish failed: {error}"));
    assert_eq!(
        subject.storage().abort(&published.session),
        Ok(()),
        "abort after publish must be a no-op"
    );
    assert_eq!(
        subject.published_media(&artifact).as_deref(),
        Some(&b"kept"[..]),
        "abort must never delete a published recording"
    );
}

/// A closed output for another reservation, or for a target storage never
/// issued, is rejected with `InvalidInput`; nothing is published and both
/// reservations stay intact and publishable with their own output.
pub fn rejects_foreign_closed_output<S: StorageSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let own = subject
        .storage()
        .begin(RecordingId::new(5), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    let other = subject
        .storage()
        .begin(RecordingId::new(6), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    subject.write_media(&own.target, b"own");
    subject.write_media(&other.target, b"other");

    let swapped = subject
        .storage()
        .publish(&own.session, ClosedOutput::new(other.target.clone()));
    assert!(
        is_invalid_input(&swapped),
        "foreign closed output: {swapped:?}"
    );
    let forged = match OutputTarget::new("never-issued/output.part") {
        Ok(target) => target,
        Err(error) => panic!("valid target rejected: {error}"),
    };
    let unknown = subject
        .storage()
        .publish(&own.session, ClosedOutput::new(forged));
    assert!(is_invalid_input(&unknown), "unissued target: {unknown:?}");
    assert!(subject.candidate_exists(&own.target), "own candidate kept");
    assert!(
        subject.candidate_exists(&other.target),
        "other candidate kept"
    );

    for (reservation, expected) in [(own, &b"own"[..]), (other, &b"other"[..])] {
        let artifact = subject
            .storage()
            .publish(
                &reservation.session,
                ClosedOutput::new(reservation.target.clone()),
            )
            .unwrap_or_else(|error| panic!("matching publish failed: {error}"));
        assert_eq!(
            subject.published_media(&artifact).as_deref(),
            Some(expected)
        );
    }
}

/// Publishing a session the adapter never issued fails with `InvalidInput`;
/// aborting it is a no-op.
pub fn rejects_unknown_session<S: StorageSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let issued = subject
        .storage()
        .begin(RecordingId::new(7), &configuration())
        .unwrap_or_else(|error| panic!("begin failed: {error}"));
    let unknown = match crate::ports::StorageSessionId::new("never-issued") {
        Ok(session) => session,
        Err(error) => panic!("valid session id rejected: {error}"),
    };
    let result = subject
        .storage()
        .publish(&unknown, ClosedOutput::new(issued.target.clone()));
    assert!(is_invalid_input(&result), "unknown session: {result:?}");
    assert!(subject.candidate_exists(&issued.target));
    assert_eq!(subject.storage().abort(&unknown), Ok(()));
    assert!(
        subject.candidate_exists(&issued.target),
        "aborting an unknown session must not touch other reservations"
    );
    assert_eq!(subject.storage().abort(&issued.session), Ok(()));
}

/// Runs every storage suite.
pub fn run_all<S: StorageSubject>(mut make: impl FnMut() -> S) {
    reservations_are_exclusive(&mut make);
    publishes_closed_output_once(&mut make);
    abort_is_idempotent_and_spares_published_output(&mut make);
    rejects_foreign_closed_output(&mut make);
    rejects_unknown_session(&mut make);
}
