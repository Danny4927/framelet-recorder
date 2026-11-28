//! On-disk schema of the settings file and its mapping to core types.
//!
//! The serde DTO is private to this module; the core settings types never
//! derive serde (ADR-0023). Decoding checks, in this order: TOML syntax, the
//! schema version, the schema shape (unknown keys, value types), and finally
//! the core invariants.

use std::num::NonZeroUsize;

use framelet_core::ports::{RecordingSettings, SettingsError};
use framelet_core::settings::{OutputDirectory, SettingsIoFailure};
use serde::{Deserialize, Serialize};

/// The only schema version this build reads and writes.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Key of the schema version; read before the rest of the file.
const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Comment written at the top of every saved file.
const HEADER: &str = "\
# Framelet settings (schema version 1), written by Framelet.
# Unknown keys or invalid values make Framelet ignore this file, report the
# problem, and use defaults; the file itself is then left unchanged.

";

/// Schema version 1. Absent optional keys take the core defaults.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SettingsFileV1 {
    schema_version: u32,
    #[serde(default)]
    microphone_enabled_by_default: bool,
    /// Absent: the platform default output directory applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_directory: Option<String>,
}

/// Returns the 1-based line containing byte `offset` of `text`.
pub(crate) fn line_at(text: &[u8], offset: usize) -> Option<NonZeroUsize> {
    let before = text.get(..offset).unwrap_or(text);
    let newlines = before.iter().filter(|byte| **byte == b'\n').count();
    NonZeroUsize::new(newlines.checked_add(1)?)
}

fn error_line(text: &str, error: &toml::de::Error) -> Option<NonZeroUsize> {
    error
        .span()
        .and_then(|span| line_at(text.as_bytes(), span.start))
}

/// Decodes a settings file into validated core settings.
pub(crate) fn decode(text: &str) -> Result<RecordingSettings, SettingsError> {
    let table: toml::Table = toml::from_str(text).map_err(|error| SettingsError::Malformed {
        line: error_line(text, &error),
    })?;
    match table.get(SCHEMA_VERSION_KEY) {
        None => return Err(SettingsError::MissingSchemaVersion),
        Some(toml::Value::Integer(found)) if *found != i64::from(SCHEMA_VERSION) => {
            return Err(SettingsError::UnsupportedSchemaVersion {
                found: *found,
                supported: SCHEMA_VERSION,
            });
        }
        // The supported version, or a value of the wrong type, which the
        // typed decoding below reports with its line.
        Some(_) => {}
    }
    let file: SettingsFileV1 =
        toml::from_str(text).map_err(|error| SettingsError::SchemaMismatch {
            line: error_line(text, &error),
        })?;
    let output_directory = file
        .output_directory
        .map(OutputDirectory::new)
        .transpose()
        .map_err(SettingsError::InvalidOutputDirectory)?;
    Ok(RecordingSettings {
        output_directory,
        microphone_enabled_by_default: file.microphone_enabled_by_default,
    })
}

/// Encodes core settings as a schema version 1 file.
pub(crate) fn encode(settings: &RecordingSettings) -> Result<String, SettingsError> {
    let file = SettingsFileV1 {
        schema_version: SCHEMA_VERSION,
        microphone_enabled_by_default: settings.microphone_enabled_by_default,
        output_directory: settings
            .output_directory
            .as_ref()
            .map(|directory| directory.as_str().to_owned()),
    };
    // Serializing two plain keys and a string cannot fail; map it anyway
    // rather than panic.
    let body =
        toml::to_string(&file).map_err(|_| SettingsError::WriteFailed(SettingsIoFailure::Other))?;
    let mut text = String::with_capacity(HEADER.len().saturating_add(body.len()));
    text.push_str(HEADER);
    text.push_str(&body);
    Ok(text)
}
