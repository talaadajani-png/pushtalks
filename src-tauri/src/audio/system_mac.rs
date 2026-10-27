//! System audio on macOS through ScreenCaptureKit (macOS 13+). Needs Screen Recording permission.

use anyhow::{anyhow, Result};
use core_graphics::access::ScreenCaptureAccess;
use screencapturekit::cm::CMTime;
use screencapturekit::prelude::*;

use super::capture::Capture;

const RATE: u32 = 48_000;

pub fn has_permission() -> bool {
    ScreenCaptureAccess.preflight()
}

/// Shows the system prompt the first time; afterwards the user must enable it in System Settings.
pub fn request_permission() -> bool {
    ScreenCaptureAccess.request()
}

struct Guard(SCStream);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.stop_capture();
    }
}

pub fn start() -> Result<Capture> {
    Capture::spawn("system", |tx| {
        let content = SCShareableContent::get().map_err(|e| {
            anyhow!("can't capture system audio (is Screen Recording allowed?): {e:?}")
        })?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no display found"))?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| anyhow!("{e:?}"))?;
        // Audio only: keep the unavoidable video stream tiny and slow.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(RATE as i32)
            .with_channel_count(1);
        let mut stream = SCStream::new(&filter, &config).map_err(|e| anyhow!("{e:?}"))?;
        stream
            .add_output_handler(
                move |sample: CMSampleBuffer, kind: SCStreamOutputType| {
                    if !matches!(kind, SCStreamOutputType::Audio) {
                        return;
                    }
                    let Ok(list) = sample.audio_buffer_list() else {
                        return;
                    };
                    // One channel requested; take the first buffer as native-endian f32.
                    if let Some(buf) = (&list).into_iter().next() {
                        let samples: Vec<f32> = buf
                            .data()
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|b| f32::from_ne_bytes(*b))
                            .collect();
                        let _ = tx.send(samples);
                    }
                },
                SCStreamOutputType::Audio,
            )
            .map_err(|e| anyhow!("{e:?}"))?;
        stream.start_capture().map_err(|e| anyhow!("{e:?}"))?;
        Ok((Guard(stream), RATE, "System audio".to_string()))
    })
}
