//! Microphone capture: cpal stream on a dedicated thread, mono mixdown,
//! linear resample to 16 kHz.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const TARGET_RATE: u32 = 16_000;

enum Cmd {
    Start {
        device: Option<String>,
        reply: Sender<Result<()>>,
    },
    Stop(Sender<Vec<f32>>),
    Cancel,
}

/// Handle to the capture thread.
pub struct AudioRecorder {
    cmd: Sender<Cmd>,
    recording: Arc<AtomicBool>,
}

impl AudioRecorder {
    /// `on_level` receives a 0..1 RMS estimate roughly every 60 ms while recording.
    pub fn new(on_level: Arc<dyn Fn(f32) + Send + Sync>) -> Self {
        let (tx, rx) = channel::<Cmd>();
        // Level updates leave the realtime callback through a channel, so the
        // UI work never runs on the audio thread.
        let (level_tx, level_rx) = channel::<f32>();
        std::thread::Builder::new()
            .name("bolum-level".into())
            .spawn(move || {
                for level in level_rx {
                    on_level(level);
                }
            })
            .expect("spawn level thread");
        let recording = Arc::new(AtomicBool::new(false));
        let audio_thread = AudioThread { level: level_tx };
        std::thread::Builder::new()
            .name("bolum-audio".into())
            .spawn(move || audio_thread.run(rx))
            .expect("spawn audio thread");
        Self {
            cmd: tx,
            recording,
        }
    }

    /// Opens the stream and waits until the device is actually running, so
    /// missing or busy microphones surface as errors instead of silent takes.
    pub fn start(&self, device: Option<String>) -> Result<()> {
        if self.recording.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("already recording"));
        }
        match self.open(device) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.recording.store(false, Ordering::SeqCst);
                Err(err)
            }
        }
    }

    fn open(&self, device: Option<String>) -> Result<()> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Start { device, reply: tx })
            .map_err(|_| anyhow!("audio thread gone"))?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| Err(anyhow!("timed out opening the microphone")))
    }

    pub fn cancel(&self) {
        self.recording.store(false, Ordering::SeqCst);
        let _ = self.cmd.send(Cmd::Cancel);
    }

    /// Stops capture and returns 16 kHz mono samples.
    pub fn stop(&self) -> Result<Vec<f32>> {
        if !self.recording.swap(false, Ordering::SeqCst) {
            return Err(anyhow!("not recording"));
        }
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Stop(tx))
            .map_err(|_| anyhow!("audio thread gone"))?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| anyhow!("timed out waiting for the microphone"))
    }
}

struct AudioThread {
    level: Sender<f32>,
}

impl AudioThread {
    fn run(self, rx: Receiver<Cmd>) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Cmd::Start { device, reply } => {
                    if let Err(err) = self.capture(&rx, device, &reply) {
                        log::error!("capture failed: {err:#}");
                    }
                }
                Cmd::Stop(reply) => {
                    let _ = reply.send(Vec::new());
                }
                Cmd::Cancel => {}
            }
        }
    }

    /// Records until Stop/Cancel arrives and replies with 16 kHz mono samples.
    fn capture(
        &self,
        rx: &Receiver<Cmd>,
        requested: Option<String>,
        reply: &Sender<Result<()>>,
    ) -> Result<()> {
        match self.capture_inner(rx, requested, reply) {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = reply.send(Err(anyhow!("{err:#}")));
                Err(err)
            }
        }
    }

    fn capture_inner(
        &self,
        rx: &Receiver<Cmd>,
        requested: Option<String>,
        reply: &Sender<Result<()>>,
    ) -> Result<()> {
        let host = cpal::default_host();
        let device = find_input_device(&host, requested.as_deref())?;
        let supported = device.default_input_config()?;
        let sample_rate = supported.sample_rate();

        let buffer: Arc<std::sync::Mutex<Vec<f32>>> = Arc::default();
        let last_emit = Arc::new(AtomicU64::new(0));

        let stream = build_stream(&device, &supported, {
            let buffer = Arc::clone(&buffer);
            let last_emit = Arc::clone(&last_emit);
            let level = self.level.clone();
            move |mono_frame: &[f32]| {
                buffer.lock().unwrap().extend_from_slice(mono_frame);
                let millis = now_millis();
                if millis.saturating_sub(last_emit.load(Ordering::Relaxed)) > 60 {
                    last_emit.store(millis, Ordering::Relaxed);
                    let rms = (mono_frame.iter().map(|s| s * s).sum::<f32>()
                        / mono_frame.len().max(1) as f32)
                        .sqrt();
                    // Unbounded channel: sending never blocks the realtime thread.
                    let _ = level.send(rms.min(1.0));
                }
            }
        })?;
        stream.play()?;
        let _ = reply.send(Ok(()));
        log::info!(
            "recording from '{}' at {sample_rate} Hz",
            device_name(&device).unwrap_or_else(|| "default".into())
        );

        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(150)) {
                Ok(Cmd::Stop(reply)) => {
                    drop(stream);
                    let samples = buffer.lock().unwrap().clone();
                    let resampled = resample(&samples, sample_rate, TARGET_RATE);
                    let _ = self.level.send(0.0);
                    let _ = reply.send(resampled);
                    return Ok(());
                }
                Ok(Cmd::Cancel) => {
                    drop(stream);
                    buffer.lock().unwrap().clear();
                    let _ = self.level.send(0.0);
                    return Ok(());
                }
                Ok(Cmd::Start { .. }) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

fn find_input_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device> {
    match name {
        Some(requested) => host
            .devices()?
            .filter(|device| device.supports_input())
            .find(|device| device_name(device).as_deref() == Some(requested))
            .ok_or_else(|| anyhow!("microphone '{requested}' not found")),
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("no input device found")),
    }
}

fn build_stream<F>(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    mut on_mono: F,
) -> Result<cpal::Stream>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    use cpal::SampleFormat;
    let config = supported.config();
    let channels = supported.channels() as usize;
    let err_fn = |err| log::error!("audio stream error: {err}");
    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _| on_mono(&mixdown(data, channels)),
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| on_mono(&mixdown(data, channels)),
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _| on_mono(&mixdown(data, channels)),
            err_fn,
            None,
        )?,
        SampleFormat::I32 => device.build_input_stream(
            config,
            move |data: &[i32], _| on_mono(&mixdown(data, channels)),
            err_fn,
            None,
        )?,
        SampleFormat::F64 => device.build_input_stream(
            config,
            move |data: &[f64], _| on_mono(&mixdown(data, channels)),
            err_fn,
            None,
        )?,
        format => return Err(anyhow!("unsupported sample format {format:?}")),
    };
    Ok(stream)
}

trait ToF32 {
    fn to_f32(self) -> f32;
}
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}
impl ToF32 for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / i32::MAX as f32
    }
}
impl ToF32 for f64 {
    fn to_f32(self) -> f32 {
        self as f32
    }
}

fn mixdown<T: Copy + ToF32>(data: &[T], channels: usize) -> Vec<f32> {
    data.chunks(channels.max(1))
        .map(|frame| frame.iter().map(|s| s.to_f32()).sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-interpolation resampler, adequate for speech at these rates.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if samples.is_empty() || from == to {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((samples.len() as f64) / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = samples[idx.min(samples.len() - 1)];
            let b = samples[(idx + 1).min(samples.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

pub fn input_devices() -> Vec<String> {
    cpal::default_host()
        .devices()
        .map(|devices| {
            devices
                .filter(|device| device.supports_input())
                .filter_map(|device| device_name(&device))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_to_the_same_rate_is_a_copy() {
        let samples = vec![0.1, -0.2, 0.3];
        assert_eq!(resample(&samples, 16_000, 16_000), samples);
    }

    #[test]
    fn resampling_scales_the_length() {
        let second = vec![0.0f32; 48_000];
        assert_eq!(resample(&second, 48_000, 16_000).len(), 16_000);
        assert_eq!(resample(&vec![0.0f32; 44_100], 44_100, 16_000).len(), 16_000);
    }

    #[test]
    fn resampling_keeps_a_constant_signal_constant() {
        let out = resample(&vec![0.5f32; 4_800], 48_000, 16_000);
        assert!(out.iter().all(|sample| (sample - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resampling_interpolates_between_samples() {
        // Doubling the rate puts a midpoint between each pair.
        assert_eq!(resample(&[0.0, 1.0], 8_000, 16_000), vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn resampling_nothing_gives_nothing() {
        assert!(resample(&[], 48_000, 16_000).is_empty());
    }

    #[test]
    fn mixdown_averages_channels_into_mono() {
        let stereo = [1.0f32, 0.0, 0.5, 0.5, -1.0, 1.0];
        assert_eq!(mixdown(&stereo, 2), vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn integer_samples_are_scaled_to_unit_range() {
        assert_eq!(mixdown(&[i16::MAX], 1), vec![1.0]);
        assert_eq!(mixdown(&[u16::MAX], 1), vec![1.0]);
        assert_eq!(mixdown(&[0u16], 1), vec![-1.0]);
    }
}
