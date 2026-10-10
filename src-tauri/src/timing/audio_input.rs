//! Audio capture via `cpal`: one input device, one channel, delivered as
//! mono `f32` blocks. The stream lives on its own thread (cpal streams
//! aren't `Send` everywhere) and stops when the `AudioCapture` is dropped.

use std::sync::mpsc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: u32,
}

pub fn list_input_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| id.clone());
            let channels = device.default_input_config().ok()?.channels() as u32;
            Some(AudioDeviceInfo { id, name, channels })
        })
        .collect()
}

/// A running capture; dropping it stops the stream.
pub struct AudioCapture {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts capturing `channel` (0-based, clamped) of `device_id` (the
/// default input when `None`). `on_block(samples, sample_rate)` runs on the
/// audio thread for every block, so it must stay quick.
pub fn start_capture(
    device_id: Option<String>,
    channel: usize,
    on_block: impl FnMut(&[f32], f32) + Send + 'static,
) -> Result<AudioCapture, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let thread = std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn(move || match open_stream(device_id, channel, on_block) {
            Ok(stream) => {
                let _ = ready_tx.send(Ok(()));
                // Hold the stream until the capture is dropped.
                let _ = stop_rx.recv();
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(AudioCapture { stop: Some(stop_tx), thread: Some(thread) }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err("The audio thread stopped unexpectedly".to_string()),
    }
}

fn open_stream(
    device_id: Option<String>,
    channel: usize,
    on_block: impl FnMut(&[f32], f32) + Send + 'static,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => {
            let parsed = id.parse().map_err(|_| format!("Unknown audio device {id}"))?;
            host.device_by_id(&parsed).ok_or_else(|| format!("Audio device {id} isn't available"))?
        }
        None => host.default_input_device().ok_or("No audio input device found")?,
    };
    let supported = device.default_input_config().map_err(|e| format!("Audio input unavailable: {e}"))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channel, on_block),
        SampleFormat::I16 => build::<i16>(&device, &config, channel, on_block),
        SampleFormat::I32 => build::<i32>(&device, &config, channel, on_block),
        SampleFormat::U16 => build::<u16>(&device, &config, channel, on_block),
        other => return Err(format!("Unsupported audio sample format {other}")),
    }?;
    stream.play().map_err(|e| format!("Couldn't start the audio input: {e}"))?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channel: usize,
    mut on_block: impl FnMut(&[f32], f32) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let channel = channel.min(channels.saturating_sub(1));
    let rate = config.sample_rate as f32;
    let mut mono: Vec<f32> = Vec::new();
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                mono.clear();
                mono.extend(data.chunks_exact(channels).map(|frame| frame[channel].to_sample::<f32>()));
                on_block(&mono, rate);
            },
            |err| eprintln!("Audio input error: {err}"),
            None,
        )
        .map_err(|e| format!("Couldn't open the audio input: {e}"))
}
