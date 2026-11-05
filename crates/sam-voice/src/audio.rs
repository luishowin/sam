//! Microphone capture and conversion to the 16 kHz mono `f32` stream that
//! transcribe-rs expects.
//!
//! cpal's default input device is opened once per listening session. The
//! callback only downmixes and forwards samples; resampling and inference run
//! on the worker thread, never on the audio callback.

use std::sync::mpsc::{self, Receiver};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::VoiceError;

/// The sample rate every speech model in this crate expects.
pub const SAMPLE_RATE: u32 = 16_000;

/// A block of mono samples captured at the device's native rate.
#[derive(Debug)]
pub(crate) struct CaptureChunk {
    pub(crate) samples: Vec<f32>,
    pub(crate) sample_rate: u32,
}

/// Messages from the audio callback to the worker thread.
#[derive(Debug)]
pub(crate) enum CaptureMessage {
    Samples(CaptureChunk),
    Error(String),
}

/// A live input stream. Dropping it (or calling [`InputCapture::stop`]) closes
/// the device.
pub(crate) struct InputCapture {
    stream: Option<cpal::Stream>,
    pub(crate) rx: Receiver<CaptureMessage>,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) device: String,
}

impl InputCapture {
    /// Stop the device. Queued callbacks remain in `rx` and can still be drained.
    pub(crate) fn stop(&mut self) {
        self.stream.take();
    }
}

/// Open the default input device and start forwarding mono samples.
pub(crate) fn start_default_input() -> Result<InputCapture, VoiceError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        VoiceError::Microphone(
            "no default input device; check that PipeWire is running and a microphone is connected"
                .into(),
        )
    })?;

    let device_id = device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "default input".into());

    let supported = device.default_input_config().map_err(|error| {
        VoiceError::Microphone(format!("no usable default input configuration: {error}"))
    })?;

    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config = supported.config();

    let (tx, rx) = mpsc::channel();
    let error_tx = tx.clone();
    let channels_for_callback = channels as usize;

    macro_rules! build_stream {
        ($sample:ty) => {{
            let samples_tx = tx.clone();
            device.build_input_stream::<$sample, _, _>(
                config.clone(),
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    let mono = downmix_to_mono(data, channels_for_callback);
                    if !mono.is_empty() {
                        let _ = samples_tx.send(CaptureMessage::Samples(CaptureChunk {
                            samples: mono,
                            sample_rate,
                        }));
                    }
                },
                move |error| {
                    let _ = error_tx.send(CaptureMessage::Error(format!("{error}")));
                },
                None,
            )
        }};
    }

    let stream = match sample_format {
        SampleFormat::I8 => build_stream!(i8),
        SampleFormat::I16 => build_stream!(i16),
        SampleFormat::I24 => build_stream!(cpal::I24),
        SampleFormat::I32 => build_stream!(i32),
        SampleFormat::I64 => build_stream!(i64),
        SampleFormat::U8 => build_stream!(u8),
        SampleFormat::U16 => build_stream!(u16),
        SampleFormat::U24 => build_stream!(cpal::U24),
        SampleFormat::U32 => build_stream!(u32),
        SampleFormat::U64 => build_stream!(u64),
        SampleFormat::F32 => build_stream!(f32),
        SampleFormat::F64 => build_stream!(f64),
        other => {
            return Err(VoiceError::Microphone(format!(
                "unsupported microphone sample format: {other}"
            )));
        }
    }
    .map_err(|error| VoiceError::Microphone(format!("could not open the input stream: {error}")))?;

    stream
        .play()
        .map_err(|error| VoiceError::Microphone(format!("could not start the input stream: {error}")))?;

    Ok(InputCapture {
        stream: Some(stream),
        rx,
        sample_rate,
        channels,
        device: device_id,
    })
}

/// Average interleaved frames down to one mono sample per frame.
pub(crate) fn downmix_to_mono<T>(data: &[T], channels: usize) -> Vec<f32>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    if channels == 0 {
        return Vec::new();
    }
    data.chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|sample| f32::from_sample(*sample)).sum();
            sum / channels as f32
        })
        .collect()
}

/// Resample mono `f32` audio to [`SAMPLE_RATE`].
///
/// This is intentionally a whole-buffer operation: the worker calls it before
/// each inference pass. It uses rubato's FFT resampler and trims its startup
/// delay, so partial and final results see the same timeline.
pub fn resample_to_16k(input: &[f32], input_rate: u32) -> Result<Vec<f32>, VoiceError> {
    if input.is_empty() || input_rate == SAMPLE_RATE {
        return Ok(input.to_vec());
    }
    if input_rate == 0 {
        return Err(VoiceError::Resample("input sample rate is zero".into()));
    }

    let mut resampler = Fft::<f32>::new(
        input_rate as usize,
        SAMPLE_RATE as usize,
        1024,
        1,
        FixedSync::Both,
    )
    .map_err(|error| VoiceError::Resample(format!("could not create resampler: {error}")))?;

    let input_buffer = InterleavedSlice::new(input, 1, input.len())
        .map_err(|error| VoiceError::Resample(format!("invalid input buffer: {error}")))?;

    let output = resampler
        .process_all(&input_buffer, input.len(), None)
        .map_err(|error| VoiceError::Resample(format!("resample failed: {error}")))?;

    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_stereo_frames() {
        let input: [i16; 6] = [1000, 3000, -1000, 1000, 0, 0];
        let mono = downmix_to_mono(&input, 2);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - (2000.0 / 32768.0)).abs() < 1e-6);
        assert!(mono[1].abs() < 1e-6);
        assert_eq!(mono[2], 0.0);
    }

    #[test]
    fn downmix_handles_mono_and_zero_channels() {
        assert_eq!(downmix_to_mono(&[0.5f32, -0.5], 1), vec![0.5, -0.5]);
        assert!(downmix_to_mono(&[0.5f32], 0).is_empty());
    }

    #[test]
    fn resample_48k_to_16k_preserves_duration() {
        let one_second = vec![0.0f32; 48_000];
        let out = resample_to_16k(&one_second, 48_000).unwrap();
        assert!(
            out.len().abs_diff(16_000) <= 2,
            "expected about 16000 samples, got {}",
            out.len()
        );
    }

    #[test]
    fn resample_44k1_to_16k_preserves_duration() {
        let half_second = vec![0.0f32; 22_050];
        let out = resample_to_16k(&half_second, 44_100).unwrap();
        assert!(
            out.len().abs_diff(8_000) <= 2,
            "expected about 8000 samples, got {}",
            out.len()
        );
    }

    #[test]
    fn resample_is_a_noop_at_16k_and_for_empty_input() {
        let input = vec![0.25f32; 10];
        assert_eq!(resample_to_16k(&input, 16_000).unwrap(), input);
        assert!(resample_to_16k(&[], 48_000).unwrap().is_empty());
        assert!(resample_to_16k(&input, 0).is_err());
    }
}
