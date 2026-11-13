//! Plays the embedded click sound through the default output device.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::assets;

#[derive(Default)]
struct Shared {
    /// Set by the UI; the audio thread rewinds to the start when it sees it.
    restart: AtomicBool,
    playing: AtomicBool,
}

pub struct Player {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    running: std::cell::Cell<bool>,
}

impl Player {
    pub fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let samples: Arc<[i16]> = miniz_oxide::inflate::decompress_to_vec(assets::SOUND)
            .ok()?
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        debug_assert_eq!(samples.len(), assets::SOUND_SAMPLES);
        let shared = Arc::new(Shared::default());

        let format = config.sample_format();
        let config: StreamConfig = config.into();
        let stream = match format {
            SampleFormat::I16 => build::<i16>(&device, &config, samples, shared.clone()),
            SampleFormat::I32 => build::<i32>(&device, &config, samples, shared.clone()),
            SampleFormat::U16 => build::<u16>(&device, &config, samples, shared.clone()),
            _ => build::<f32>(&device, &config, samples, shared.clone()),
        }
        .map_err(|err| eprintln!("audio unavailable: {err}"))
        .ok()?;
        Some(Self { stream, shared, running: false.into() })
    }

    /// Plays the sound from the start, cutting off the current one.
    pub fn restart(&self) {
        self.shared.playing.store(true, Ordering::Release);
        self.shared.restart.store(true, Ordering::Release);
        if !self.running.replace(true) {
            if let Err(err) = self.stream.play() {
                eprintln!("failed to start audio: {err}");
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    /// Pauses the device between clicks so the app is idle when not in use.
    pub fn idle(&self) {
        if self.running.replace(false) {
            let _ = self.stream.pause();
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Arc<[i16]>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let step = assets::SOUND_RATE as f64 / config.sample_rate as f64;
    let mut pos = f64::INFINITY;
    device.build_output_stream(
        config.clone(),
        move |out: &mut [T], _| {
            if shared.restart.swap(false, Ordering::AcqRel) {
                pos = 0.0;
            }
            for frame in out.chunks_mut(channels) {
                // Linear resampling to the device rate.
                let i = pos as usize;
                let value = if i + 1 < samples.len() {
                    let t = (pos - i as f64) as f32;
                    let (a, b) = (samples[i] as f32, samples[i + 1] as f32);
                    pos += step;
                    (a + (b - a) * t) / i16::MAX as f32
                } else {
                    0.0
                };
                frame.fill(T::from_sample(value));
            }
            if pos as usize + 1 >= samples.len() {
                shared.playing.store(false, Ordering::Release);
            }
        },
        |err| eprintln!("audio error: {err}"),
        None,
    )
}
