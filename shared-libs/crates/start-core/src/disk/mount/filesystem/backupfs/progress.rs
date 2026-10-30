use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use backup_fs_protocol::{ConversionPhase, ConversionProgress, MAX_PROGRESS_LINE};
use tokio::io::AsyncWrite;

use crate::prelude::*;
use crate::progress::{FullProgressTracker, PhaseProgressTrackerHandle, Progress, ProgressUnits};

#[cfg(test)]
mod tests;

pub(super) struct ProgressSink {
    buffer: Vec<u8>,
    tracker: FullProgressTracker,
    current: Option<(ConversionPhase, PhaseProgressTrackerHandle)>,
    complete: bool,
}

impl ProgressSink {
    pub fn new(tracker: &FullProgressTracker) -> Self {
        Self {
            buffer: Vec::new(),
            tracker: tracker.clone(),
            current: None,
            complete: false,
        }
    }

    fn update(&mut self, event: ConversionProgress) -> io::Result<()> {
        if self.complete {
            return Err(io::Error::other(
                t!("backup.conversion.invalid-progress").to_string(),
            ));
        }
        if event.phase == ConversionPhase::Complete {
            if let Some((_, phase)) = &mut self.current {
                phase.complete();
            }
            self.tracker.complete();
            self.complete = true;
            return Ok(());
        }
        if self.current.as_ref().map(|(phase, _)| *phase) != Some(event.phase) {
            if let Some((_, phase)) = &mut self.current {
                phase.complete();
            }
            let name = match event.phase {
                ConversionPhase::Inspecting => t!("backup.conversion.inspecting"),
                ConversionPhase::Records => t!("backup.conversion.records"),
                ConversionPhase::ScanningObjects => t!("backup.conversion.scanning-objects"),
                ConversionPhase::Objects => t!("backup.conversion.objects"),
                ConversionPhase::Publishing => t!("backup.conversion.publishing"),
                ConversionPhase::Complete => unreachable!(),
            };
            let mut phase = self.tracker.add_phase(name.as_ref().into(), None);
            phase.start();
            self.current = Some((event.phase, phase));
        }
        self.current
            .as_mut()
            .unwrap()
            .1
            .set_phase_value(Progress::Progress {
                done: event.done,
                total: event.total,
                units: Some(if event.phase == ConversionPhase::Records {
                    ProgressUnits::Bytes
                } else {
                    ProgressUnits::Steps
                }),
            });
        Ok(())
    }

    fn finish(&self) -> io::Result<()> {
        if !self.buffer.is_empty() || !self.complete {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                t!("backup.conversion.incomplete-progress").to_string(),
            ));
        }
        Ok(())
    }
}

impl AsyncWrite for ProgressSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        for &byte in bytes {
            if byte == b'\n' {
                let event = match serde_json::from_slice(&self.buffer) {
                    Ok(event) => event,
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::other(
                            t!("backup.conversion.invalid-progress").to_string(),
                        )));
                    }
                };
                self.buffer.clear();
                if let Err(error) = self.update(event) {
                    return Poll::Ready(Err(error));
                }
            } else {
                if self.buffer.len() >= MAX_PROGRESS_LINE {
                    return Poll::Ready(Err(io::Error::other(
                        t!("backup.conversion.progress-limit").to_string(),
                    )));
                }
                self.buffer.push(byte);
            }
        }
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.finish())
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.finish())
    }
}
