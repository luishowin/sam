//! WAV loading for the spike CLI and for tests.
//!
//! The model wants 16 kHz mono `f32`. The reader accepts the WAV formats hound
//! can decode and converts them here so a normal recording can be dropped in
//! without an external conversion step.

use std::path::Path;
use std::time::Duration;

use crate::audio::{downmix_to_mono, resample_to_16k, SAMPLE_RATE};
use crate::VoiceError;

/// A WAV file converted to the model's input format.
#[derive(Debug, Clone)]
pub struct WavAudio {
    /// Mono samples at 16 kHz, normalised to roughly `[-1, 1]`.
    pub samples: Vec<f32>,
    /// Sample rate of the file that was read.
    pub source_sample_rate: u32,
    /// Channel count of the file that was read.
    pub source_channels: u16,
    /// Duration of the file that was read.
    pub source_duration: Duration,
}

impl WavAudio {
    /// Duration of the converted, 16 kHz audio.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / SAMPLE_RATE as f64)
    }
}

/// Read a WAV file, downmix it to mono and resample it to 16 kHz.
pub fn read_wav_16k_mono(path: &Path) -> Result<WavAudio, VoiceError> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|error| VoiceError::Wav(format!("{}: {error}", path.display())))?;
    let spec = reader.spec();

    if spec.channels == 0 {
        return Err(VoiceError::Wav("file has zero channels".into()));
    }
    if spec.sample_rate == 0 {
        return Err(VoiceError::Wav("file has a zero sample rate".into()));
    }

    let interleaved = read_interleaved(&mut reader, &spec)?;
    let frames = interleaved.len() / spec.channels as usize;
    let source_duration = Duration::from_secs_f64(frames as f64 / spec.sample_rate as f64);

    let mono = downmix_to_mono(&interleaved, spec.channels as usize);
    let samples = resample_to_16k(&mono, spec.sample_rate)?;

    Ok(WavAudio {
        samples,
        source_sample_rate: spec.sample_rate,
        source_channels: spec.channels,
        source_duration,
    })
}

fn read_interleaved(
    reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>,
    spec: &hound::WavSpec,
) -> Result<Vec<f32>, VoiceError> {
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 8) => collect_samples(reader, |value: i8| {
            value as f32 / 128.0
        }),
        (hound::SampleFormat::Int, 16) => collect_samples(reader, |value: i16| {
            value as f32 / 32_768.0
        }),
        (hound::SampleFormat::Int, 24 | 32) => collect_samples(reader, |value: i32| {
            value as f32 / 2_147_483_648.0
        }),
        (hound::SampleFormat::Float, 32) => collect_samples(reader, |value: f32| value),
        (format, bits) => Err(VoiceError::Wav(format!(
            "unsupported WAV format: {format:?}, {bits} bits per sample"
        ))),
    }
}

fn collect_samples<S, F>(
    reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>,
    convert: F,
) -> Result<Vec<f32>, VoiceError>
where
    S: hound::Sample,
    F: Fn(S) -> f32,
{
    reader
        .samples::<S>()
        .map(|sample| {
            sample
                .map(|value| convert(value))
                .map_err(|error| VoiceError::Wav(error.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn write_wav(path: &Path, sample_rate: u32, channels: u16, frames: usize) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for frame in 0..frames {
            for channel in 0..channels {
                let value = ((frame as i32 * 31 + channel as i32 * 7) % 2000 - 1000) as i16;
                writer.write_sample(value).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn reads_16k_mono_without_changing_length() {
        let temp = TempDir::new("wav-mono");
        let path = temp.path().join("mono.wav");
        write_wav(&path, 16_000, 1, 1_600);

        let audio = read_wav_16k_mono(&path).unwrap();
        assert_eq!(audio.samples.len(), 1_600);
        assert_eq!(audio.source_sample_rate, 16_000);
        assert_eq!(audio.source_channels, 1);
        assert_eq!(audio.source_duration, Duration::from_millis(100));
    }

    #[test]
    fn downmixes_stereo_48k_and_resamples() {
        let temp = TempDir::new("wav-stereo");
        let path = temp.path().join("stereo.wav");
        write_wav(&path, 48_000, 2, 4_800);

        let audio = read_wav_16k_mono(&path).unwrap();
        assert_eq!(audio.source_channels, 2);
        assert!(
            audio.samples.len().abs_diff(1_600) <= 2,
            "expected about 1600 samples, got {}",
            audio.samples.len()
        );
        assert!(audio.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn missing_or_bad_wav_is_an_error_not_a_panic() {
        let temp = TempDir::new("wav-bad");
        assert!(read_wav_16k_mono(&temp.path().join("nope.wav")).is_err());

        let junk = temp.path().join("junk.wav");
        std::fs::write(&junk, b"not a wav").unwrap();
        assert!(read_wav_16k_mono(&junk).is_err());
    }
}
