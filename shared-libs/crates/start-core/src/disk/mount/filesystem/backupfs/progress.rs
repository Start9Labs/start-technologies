use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use backup_fs_protocol::{ConversionPhase, ConversionProgress, MAX_PROGRESS_LINE};
use tokio::io::AsyncWrite;

use crate::prelude::*;
use crate::progress::{FullProgressTracker, PhaseProgressTrackerHandle, Progress, ProgressUnits};

#[cfg(test)]
mod tests;

pub struct ProgressSink {
    buffer: Vec<u8>,
    tracker: FullProgressTracker,
    phases: Vec<(ConversionPhase, PhaseProgressTrackerHandle)>,
    current: Option<usize>,
    complete: bool,
}

impl ProgressSink {
    pub fn new() -> Self {
        let tracker = FullProgressTracker::new();
        let phases = [
            (
                ConversionPhase::Inspecting,
                t!("backup.conversion.inspecting"),
                5,
            ),
            (
                ConversionPhase::Records,
                t!("backup.conversion.records"),
                20,
            ),
            (
                ConversionPhase::ScanningObjects,
                t!("backup.conversion.scanning-objects"),
                10,
            ),
            (
                ConversionPhase::Objects,
                t!("backup.conversion.objects"),
                60,
            ),
            (
                ConversionPhase::Publishing,
                t!("backup.conversion.publishing"),
                5,
            ),
        ]
        .into_iter()
        .map(|(phase, name, weight)| (phase, tracker.add_phase(name.as_ref().into(), Some(weight))))
        .collect();
        Self {
            buffer: Vec::new(),
            tracker,
            phases,
            current: None,
            complete: false,
        }
    }

    pub fn tracker(&self) -> &FullProgressTracker {
        &self.tracker
    }

    fn update(&mut self, event: ConversionProgress) -> io::Result<()> {
        if self.complete {
            return Err(io::Error::other(
                t!("backup.conversion.invalid-progress").to_string(),
            ));
        }
        if event.phase == ConversionPhase::Complete {
            for (_, phase) in &mut self.phases {
                phase.complete();
            }
            self.tracker.complete();
            self.complete = true;
            return Ok(());
        }
        let index = self
            .phases
            .iter()
            .position(|(phase, _)| *phase == event.phase)
            .unwrap();
        if self.current != Some(index) {
            for (_, phase) in &mut self.phases[..index] {
                phase.complete();
            }
            self.phases[index].1.start();
            self.current = Some(index);
        }
        self.phases[index].1.set_phase_value(Progress::Progress {
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
