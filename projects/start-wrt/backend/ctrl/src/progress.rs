use std::time::Duration;

use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

// ── Progress enum ────────────────────────────────────────────────────

/// Progress state for a single phase or overall operation.
/// Wire-compatible with start-os's `Progress` (serialized as untagged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Progress {
    /// Not started yet — serializes as `null`
    NotStarted(()),
    /// Complete — serializes as `true` (success) or `false` (failure)
    Complete(bool),
    /// In progress
    Progress {
        done: u64,
        total: Option<u64>,
        units: Option<ProgressUnits>,
    },
}

impl Progress {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }

    /// Add to the `total` of an in-progress value, or transition `NotStarted`
    /// into `Progress { done: 0, total: Some(total) }`.
    pub fn add_total(&mut self, total: u64) {
        match self {
            Self::Progress { total: Some(old), .. } => *old += total,
            _ => {
                *self = Self::Progress {
                    done: 0,
                    total: Some(total),
                    units: None,
                }
            }
        }
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::NotStarted(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressUnits {
    Bytes,
    Steps,
}

// ── FullProgress ─────────────────────────────────────────────────────

/// Snapshot of overall + per-phase progress, sent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullProgress {
    pub overall: Progress,
    pub phases: Vec<NamedProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedProgress {
    pub name: String,
    pub progress: Progress,
}

// ── FullProgressTracker ──────────────────────────────────────────────

/// Manages progress for a multi-phase operation using tokio::watch channels.
/// Port of start-os's `FullProgressTracker`, without the DB sync.
#[derive(Clone)]
pub struct FullProgressTracker {
    overall: watch::Sender<Progress>,
    phases: watch::Sender<Vec<(String, watch::Receiver<Progress>)>>,
}

impl FullProgressTracker {
    pub fn new() -> Self {
        Self {
            overall: watch::Sender::new(Progress::default()),
            phases: watch::Sender::new(Vec::new()),
        }
    }

    /// Add a new phase. `overall_contribution` is the weight of this phase
    /// relative to other phases for computing overall progress.
    pub fn add_phase(
        &self,
        name: String,
        overall_contribution: Option<u64>,
    ) -> PhaseProgressTrackerHandle {
        let (tx, rx) = watch::channel(Progress::default());
        self.phases.send_modify(|phases| {
            phases.push((name.clone(), rx));
        });
        // Bump the overall's total so it transitions NotStarted → Progress.
        // Without this, update_overall's pattern match never fires and the
        // overall channel never changes, leaving the stream stuck.
        if let Some(contribution) = overall_contribution {
            self.overall.send_modify(|p| p.add_total(contribution));
        }
        PhaseProgressTrackerHandle {
            name,
            overall: self.overall.clone(),
            overall_contribution,
            contributed: 0,
            progress: tx,
        }
    }

    /// Take a snapshot of current progress.
    pub fn snapshot(&self) -> FullProgress {
        let phases = self.phases.borrow();
        FullProgress {
            overall: *self.overall.borrow(),
            phases: phases
                .iter()
                .map(|(name, rx)| NamedProgress {
                    name: name.clone(),
                    progress: *rx.borrow(),
                })
                .collect(),
        }
    }

    /// Stream progress snapshots, throttled to at most one per `min_interval`.
    ///
    /// The first yielded snapshot is the current state at subscription time;
    /// subsequent snapshots are yielded when the overall progress channel
    /// changes.
    pub fn stream(self, min_interval: Option<Duration>) -> BoxStream<'static, FullProgress> {
        #[derive(Clone, Copy)]
        enum State {
            First,
            Waiting,
            Done,
        }
        let overall_rx = self.overall.subscribe();

        let stream = futures_util::stream::unfold(
            (self, overall_rx, State::First),
            move |(tracker, mut rx, state)| async move {
                match state {
                    State::Done => None,
                    State::First => {
                        let snapshot = tracker.snapshot();
                        let next = if snapshot.overall.is_complete() {
                            State::Done
                        } else {
                            State::Waiting
                        };
                        Some((snapshot, (tracker, rx, next)))
                    }
                    State::Waiting => match rx.changed().await {
                        Ok(()) => {
                            if let Some(interval) = min_interval {
                                tokio::time::sleep(interval).await;
                            }
                            let snapshot = tracker.snapshot();
                            let next = if snapshot.overall.is_complete() {
                                State::Done
                            } else {
                                State::Waiting
                            };
                            Some((snapshot, (tracker, rx, next)))
                        }
                        Err(_) => {
                            // Sender dropped — emit a final snapshot then stop
                            let snapshot = tracker.snapshot();
                            Some((snapshot, (tracker, rx, State::Done)))
                        }
                    },
                }
            },
        );

        Box::pin(stream)
    }

    /// Mark the entire operation as successfully complete.
    pub fn complete(&self) {
        self.overall.send_replace(Progress::Complete(true));
    }

    /// Mark the entire operation as failed.
    pub fn fail(&self) {
        self.overall.send_replace(Progress::Complete(false));
    }
}

// ── PhaseProgressTrackerHandle ───────────────────────────────────────

/// Mutable handle for tracking progress of a single phase.
pub struct PhaseProgressTrackerHandle {
    name: String,
    overall: watch::Sender<Progress>,
    overall_contribution: Option<u64>,
    contributed: u64,
    progress: watch::Sender<Progress>,
}

impl PhaseProgressTrackerHandle {
    pub fn start(&mut self) {
        self.progress.send_replace(Progress::Progress {
            done: 0,
            total: None,
            units: None,
        });
    }

    pub fn set_total(&mut self, total: u64) {
        self.progress.send_modify(|p| {
            if let Progress::Progress { total: t, .. } = p {
                *t = Some(total);
            }
        });
    }

    pub fn set_units(&mut self, units: Option<ProgressUnits>) {
        self.progress.send_modify(|p| {
            if let Progress::Progress { units: u, .. } = p {
                *u = units;
            }
        });
    }

    pub fn set_done(&mut self, done: u64) {
        self.progress.send_modify(|p| {
            if let Progress::Progress { done: d, .. } = p {
                *d = done;
            }
        });
        self.update_overall();
    }

    pub fn complete(&mut self) {
        self.progress.send_replace(Progress::Complete(true));
        // Flush any remaining weight to the overall tracker. update_overall
        // reads the now-Complete phase as fraction 1.0 and applies the delta
        // itself; pre-assigning `contributed` here would make its change
        // check (`new_contributed != self.contributed`) fail and skip it.
        self.update_overall();
    }

    fn update_overall(&mut self) {
        let Some(contribution) = self.overall_contribution else {
            return;
        };
        let progress = *self.progress.borrow();
        let fraction = match progress {
            Progress::Complete(_) => 1.0,
            Progress::Progress {
                done, total: Some(total), ..
            } if total > 0 => (done as f64) / (total as f64),
            _ => 0.0,
        };
        let new_contributed = (fraction * contribution as f64) as u64;
        if new_contributed != self.contributed {
            let delta = new_contributed.saturating_sub(self.contributed);
            self.contributed = new_contributed;
            self.overall.send_modify(|p| {
                if let Progress::Progress { done, .. } = p {
                    *done += delta;
                }
            });
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn overall_done(tracker: &FullProgressTracker) -> u64 {
        match tracker.snapshot().overall {
            Progress::Progress { done, .. } => done,
            other => panic!("expected in-progress overall, got {other:?}"),
        }
    }

    #[test]
    fn complete_flushes_phase_weight_to_overall() {
        let tracker = FullProgressTracker::new();
        let mut a = tracker.add_phase("a".into(), Some(100));
        let mut b = tracker.add_phase("b".into(), Some(10));
        let mut c = tracker.add_phase("c".into(), Some(1));

        // Phases that only start + complete (no set_total/set_done — the
        // verify/apply case) must still contribute their full weight. This
        // is what the old complete() got wrong by pre-assigning `contributed`.
        a.start();
        a.complete();
        b.start();
        b.complete();
        c.start();
        c.complete();

        assert_eq!(overall_done(&tracker), 111);
    }

    #[test]
    fn partial_progress_advances_overall() {
        let tracker = FullProgressTracker::new();
        let mut dl = tracker.add_phase("download".into(), Some(100));

        // Mirror update_system's corrected ordering: start() before
        // set_total, otherwise set_total is a no-op on a NotStarted phase.
        dl.start();
        dl.set_total(200);
        dl.set_done(100); // halfway → 50 of the phase's 100 weight
        assert_eq!(overall_done(&tracker), 50);

        dl.complete(); // remaining weight flushed
        assert_eq!(overall_done(&tracker), 100);
    }
}
