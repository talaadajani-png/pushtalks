//! A running capture: an OS audio stream owned by its own thread, delivering 16 kHz mono chunks.
//! Resampling happens on that thread, never in the real-time audio callback.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use super::resample::StreamResampler;

/// Mono samples at the device's own rate, sent from the OS callback.
pub type RawSender = Sender<Vec<f32>>;

pub struct Capture {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    /// 16 kHz mono chunks. Closed when the capture stops.
    pub chunks: Receiver<Vec<f32>>,
    level: Arc<AtomicU32>,
    pub device: String,
}

impl Capture {
    /// `open` runs on the capture thread. It starts the OS stream, which must send mono samples
    /// to the given sender, and returns a guard that keeps the stream alive plus its sample rate
    /// and a device name.
    pub fn spawn<G: 'static>(
        name: &str,
        open: impl FnOnce(RawSender) -> Result<(G, u32, String)> + Send + 'static,
    ) -> Result<Self> {
        let (raw_tx, raw_rx) = unbounded::<Vec<f32>>();
        let (out_tx, out_rx) = unbounded::<Vec<f32>>();
        let (stop_tx, stop_rx) = bounded::<()>(1);
        let (ready_tx, ready_rx) = bounded::<Result<String>>(1);
        let level = Arc::new(AtomicU32::new(0));
        let level2 = level.clone();
        let thread = std::thread::Builder::new()
            .name(format!("capture-{name}"))
            .spawn(move || {
                let (guard, rate, device) = match open(raw_tx) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut rs = match StreamResampler::new(rate) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(device));
                loop {
                    crossbeam_channel::select! {
                        recv(raw_rx) -> msg => match msg {
                            Ok(raw) => {
                                level2.store(rms(&raw).to_bits(), Ordering::Relaxed);
                                let out = rs.push(&raw);
                                if !out.is_empty() && out_tx.send(out).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        },
                        recv(stop_rx) -> _ => break,
                    }
                }
                drop(guard);
                // Drain anything the callback sent before the stream stopped.
                while let Ok(raw) = raw_rx.try_recv() {
                    let _ = out_tx.send(rs.push(&raw));
                }
                let tail = rs.finish();
                if !tail.is_empty() {
                    let _ = out_tx.send(tail);
                }
            })?;
        let device = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| anyhow!("the {name} audio device didn't start"))??;
        Ok(Self {
            stop: Some(stop_tx),
            thread: Some(thread),
            chunks: out_rx,
            level,
            device,
        })
    }

    /// Recent input level, 0.0 to about 1.0, for the UI meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Stops the stream. Chunks already captured stay readable on `chunks`.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}
