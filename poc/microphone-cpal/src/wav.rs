#![forbid(unsafe_code)]
//! Minimal local WAV writer for bounded PCM capture PoCs.

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use framelet_capture_api::AudioBlock;

const MAX_DATA_BYTES: u64 = u32::MAX as u64 - 36;

/// Errors produced while writing and finalizing a PCM WAV recording.
#[derive(Debug)]
pub enum WavWriterError {
    /// Unsupported WAV configuration.
    UnsupportedFormat(&'static str),
    /// An incoming block does not match the configured stream.
    MismatchedBlock(&'static str),
    /// RIFF's 32-bit size limit would be exceeded.
    FileTooLarge,
    /// A filesystem operation failed.
    Io(io::Error),
}

impl std::fmt::Display for WavWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat(message) => write!(f, "unsupported WAV format: {message}"),
            Self::MismatchedBlock(message) => write!(f, "audio block mismatch: {message}"),
            Self::FileTooLarge => f.write_str("recording exceeds the RIFF/WAVE size limit"),
            Self::Io(error) => write!(f, "WAV filesystem operation failed: {error}"),
        }
    }
}

impl std::error::Error for WavWriterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for WavWriterError {
    fn from(error: io::Error) -> Self { Self::Io(error) }
}

/// Writes interleaved IEEE-float samples to a 32-bit float WAV file.
pub struct WavWriter {
    file: File,
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
    data_bytes: u64,
}

impl WavWriter {
    /// Creates a file exclusively. Existing files are never overwritten.
    pub fn create(path: impl AsRef<Path>, sample_rate: u32, channels: u16) -> Result<Self, WavWriterError> {
        if sample_rate == 0 || channels == 0 {
            return Err(WavWriterError::UnsupportedFormat("sample rate and channels must be non-zero"));
        }
        let align = channels.checked_mul(4).ok_or(WavWriterError::UnsupportedFormat("block alignment overflow"))?;
        let byte_rate = sample_rate.checked_mul(u32::from(align)).ok_or(WavWriterError::UnsupportedFormat("byte rate overflow"))?;
        let path = path.as_ref().to_path_buf();
        let mut writer = Self {
            file: OpenOptions::new().read(true).write(true).create_new(true).open(&path)?,
            path,
            sample_rate,
            channels,
            data_bytes: 0,
        };
        writer.write_header(align, byte_rate)?;
        Ok(writer)
    }

    /// Returns whether the file has exceeded the supported RIFF size limit.
    #[must_use]
    pub fn remaining_capacity_bytes(&self) -> u64 {
        MAX_DATA_BYTES.saturating_sub(self.data_bytes)
    }

    /// Appends one block, validating its negotiated format.
    pub fn write_block(&mut self, block: &AudioBlock) -> Result<(), WavWriterError> {
        if block.sample_rate != self.sample_rate || block.channels != self.channels {
            return Err(WavWriterError::MismatchedBlock("sample rate or channel count changed"));
        }
        let bytes = u64::try_from(block.samples.len()).ok()
            .and_then(|count| count.checked_mul(4)).ok_or(WavWriterError::FileTooLarge)?;
        let new_size = self.data_bytes.checked_add(bytes)
            .filter(|size| *size <= MAX_DATA_BYTES).ok_or(WavWriterError::FileTooLarge)?;
        for sample in &block.samples {
            self.file.write_all(&sample.to_le_bytes())?;
        }
        self.data_bytes = new_size;
        Ok(())
    }

    /// Patches chunk sizes, flushes and syncs, then returns the completed path.
    pub fn finalize(mut self) -> Result<PathBuf, WavWriterError> {
        let riff_size = u32::try_from(36 + self.data_bytes).map_err(|_| WavWriterError::FileTooLarge)?;
        let data_size = u32::try_from(self.data_bytes).map_err(|_| WavWriterError::FileTooLarge)?;
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&riff_size.to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&data_size.to_le_bytes())?;
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(self.path)
    }

    fn write_header(&mut self, align: u16, byte_rate: u32) -> Result<(), WavWriterError> {
        self.file.write_all(b"RIFF")?;
        self.file.write_all(&0_u32.to_le_bytes())?;
        self.file.write_all(b"WAVEfmt ")?;
        self.file.write_all(&16_u32.to_le_bytes())?;
        self.file.write_all(&3_u16.to_le_bytes())?;
        self.file.write_all(&self.channels.to_le_bytes())?;
        self.file.write_all(&self.sample_rate.to_le_bytes())?;
        self.file.write_all(&byte_rate.to_le_bytes())?;
        self.file.write_all(&align.to_le_bytes())?;
        self.file.write_all(&32_u16.to_le_bytes())?;
        self.file.write_all(b"data")?;
        self.file.write_all(&0_u32.to_le_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn path(label: &str) -> PathBuf {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).map(|v| v.as_nanos()).unwrap_or_default();
        std::env::temp_dir().join(format!("framelet-{label}-{}-{id}.wav", std::process::id()))
    }
    use std::time::{SystemTime, UNIX_EPOCH};

    fn block(rate: u32, channels: u16, samples: Vec<f32>) -> AudioBlock {
        AudioBlock::new(rate, channels, Duration::ZERO, Some(0), samples).expect("valid test block")
    }

    #[test]
    fn writes_float_wav_and_patches_sizes() {
        let output = path("valid");
        let mut writer = WavWriter::create(&output, 48_000, 2).expect("create");
        writer.write_block(&block(48_000, 2, vec![0.25, -0.5])).expect("append");
        writer.finalize().expect("finalize");
        let bytes = std::fs::read(&output).expect("read");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().expect("riff size")), 44);
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().expect("format")), 3);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().expect("data size")), 8);
        assert_eq!(&bytes[44..48], &0.25_f32.to_le_bytes());
        assert_eq!(&bytes[48..52], &(-0.5_f32).to_le_bytes());
        std::fs::remove_file(output).expect("cleanup");
    }

    #[test]
    fn never_overwrites_existing_files() {
        let output = path("existing");
        std::fs::write(&output, b"keep").expect("seed");
        let result = WavWriter::create(&output, 48_000, 1);
        assert!(matches!(result, Err(WavWriterError::Io(ref e)) if e.kind() == io::ErrorKind::AlreadyExists));
        assert_eq!(std::fs::read(&output).expect("read seed"), b"keep");
        std::fs::remove_file(output).expect("cleanup");
    }

    #[test]
    fn rejects_a_changed_input_format() {
        let output = path("format");
        let mut writer = WavWriter::create(&output, 48_000, 1).expect("create");
        assert!(matches!(writer.write_block(&block(44_100, 1, vec![0.0])), Err(WavWriterError::MismatchedBlock(_))));
        drop(writer);
        std::fs::remove_file(output).expect("cleanup");
    }
}
