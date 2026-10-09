use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::HealthCheckId;
use crate::error::ErrorData;
use crate::prelude::*;
use crate::service::start_stop::StartStop;
use crate::status::health_check::NamedHealthCheckResult;

pub mod health_check;

#[derive(Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct StatusInfo {
    pub health: BTreeMap<HealthCheckId, NamedHealthCheckResult>,
    pub error: Option<ErrorData>,
    #[ts(type = "string | null")]
    pub started: Option<DateTime<Utc>>,
    pub desired: DesiredStatus,
    #[serde(default)]
    #[ts(type = "string | null")]
    pub force_stop_at: Option<DateTime<Utc>>,
}
impl StatusInfo {
    pub fn can_force_stop(&self, deadline: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        self.desired == DesiredStatus::Stopped
            && self.started.is_some()
            && self.force_stop_at == Some(deadline)
            && now >= deadline
    }

    pub fn sync_force_stop_deadline(&mut self, now: DateTime<Utc>, delay: std::time::Duration) {
        if self.desired == DesiredStatus::Stopped && self.started.is_some() {
            self.force_stop_at.get_or_insert_with(|| {
                chrono::TimeDelta::from_std(delay)
                    .ok()
                    .and_then(|delay| now.checked_add_signed(delay))
                    .unwrap_or(DateTime::<Utc>::MAX_UTC)
            });
        } else {
            self.force_stop_at = None;
        }
    }

    pub fn stop(&mut self) {
        self.desired = self.desired.stop();
        self.health.clear();
    }
}
impl Model<StatusInfo> {
    pub fn start(&mut self) -> Result<(), Error> {
        self.as_force_stop_at_mut().ser(&None)?;
        self.as_desired_mut().map_mutate(|s| Ok(s.start()))?;
        Ok(())
    }
    pub fn started(&mut self) -> Result<(), Error> {
        self.as_force_stop_at_mut().ser(&None)?;
        self.as_started_mut()
            .map_mutate(|s| Ok(Some(s.unwrap_or_else(|| Utc::now()))))?;
        self.as_desired_mut().map_mutate(|s| Ok(s.started()))?;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        self.as_desired_mut().map_mutate(|s| Ok(s.stop()))?;
        self.as_health_mut().ser(&Default::default())?;
        Ok(())
    }
    pub fn stopped(&mut self) -> Result<(), Error> {
        self.as_force_stop_at_mut().ser(&None)?;
        self.as_started_mut().ser(&None)?;
        self.as_health_mut().ser(&Default::default())?;
        Ok(())
    }
    pub fn restart(&mut self) -> Result<(), Error> {
        self.as_force_stop_at_mut().ser(&None)?;
        let started = self.as_started().transpose_ref().is_some();
        self.as_desired_mut()
            .map_mutate(|s| Ok(s.restart(started)))?;
        self.as_health_mut().ser(&Default::default())?;
        Ok(())
    }
    pub fn init(&mut self) -> Result<(), Error> {
        self.stopped()?;
        self.as_desired_mut().map_mutate(|s| {
            Ok(match s {
                DesiredStatus::BackingUp {
                    on_complete: StartStop::Start,
                }
                | DesiredStatus::Updating {
                    on_complete: StartStop::Start,
                } => DesiredStatus::Running,
                DesiredStatus::BackingUp {
                    on_complete: StartStop::Stop,
                }
                | DesiredStatus::Updating {
                    on_complete: StartStop::Stop,
                } => DesiredStatus::Stopped,
                DesiredStatus::Restarting { .. } => DesiredStatus::Running,
                x => x,
            })
        })?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(tag = "main")]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "camelCase")]
pub enum DesiredStatus {
    Stopped,
    Restarting {
        #[serde(default)]
        restart_again: bool,
    },
    Running,
    BackingUp {
        on_complete: StartStop,
    },
    Updating {
        on_complete: StartStop,
    },
}
impl Default for DesiredStatus {
    fn default() -> Self {
        Self::Stopped
    }
}
impl DesiredStatus {
    pub fn running(&self) -> bool {
        match self {
            Self::Running
            | Self::Restarting { .. }
            | Self::BackingUp {
                on_complete: StartStop::Start,
            }
            | Self::Updating {
                on_complete: StartStop::Start,
            } => true,
            Self::Stopped
            | Self::BackingUp {
                on_complete: StartStop::Stop,
            }
            | Self::Updating {
                on_complete: StartStop::Stop,
            } => false,
        }
    }
    pub fn run_state(&self) -> StartStop {
        if self.running() {
            StartStop::Start
        } else {
            StartStop::Stop
        }
    }

    pub fn backing_up(&self) -> Self {
        Self::BackingUp {
            on_complete: self.run_state(),
        }
    }

    pub fn updating(&self) -> Self {
        Self::Updating {
            on_complete: self.run_state(),
        }
    }

    pub fn stop(&self) -> Self {
        match self {
            Self::BackingUp { .. } => Self::BackingUp {
                on_complete: StartStop::Stop,
            },
            Self::Updating { .. } => Self::Updating {
                on_complete: StartStop::Stop,
            },
            _ => Self::Stopped,
        }
    }

    pub fn start(&self) -> Self {
        match self {
            Self::BackingUp { .. } => Self::BackingUp {
                on_complete: StartStop::Start,
            },
            Self::Updating { .. } => Self::Updating {
                on_complete: StartStop::Start,
            },
            Self::Stopped => Self::Running,
            x => *x,
        }
    }

    /// The desired status once a start completes.
    pub fn started(&self) -> Self {
        match self {
            Self::Restarting {
                restart_again: true,
            } => Self::Restarting {
                restart_again: false,
            },
            Self::Restarting {
                restart_again: false,
            } => Self::Running,
            x => *x,
        }
    }

    pub fn restart(&self, started: bool) -> Self {
        match self {
            Self::Running => Self::Restarting {
                restart_again: !started,
            },
            Self::Restarting { .. } if !started => Self::Restarting {
                restart_again: true,
            },
            x => *x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_stop_eligibility_is_bound_to_the_stop_episode() {
        let deadline = Utc::now();
        let mut status = StatusInfo {
            started: Some(deadline - chrono::TimeDelta::seconds(60)),
            force_stop_at: Some(deadline),
            ..Default::default()
        };
        assert!(status.can_force_stop(deadline, deadline));
        assert!(!status.can_force_stop(deadline, deadline - chrono::TimeDelta::milliseconds(1)));
        assert!(!status.can_force_stop(deadline - chrono::TimeDelta::seconds(1), deadline));
        status.desired = DesiredStatus::Running;
        assert!(!status.can_force_stop(deadline, deadline));
        status.desired = DesiredStatus::Restarting {
            restart_again: false,
        };
        assert!(!status.can_force_stop(deadline, deadline));
        status.desired = DesiredStatus::Stopped;
        status.started = None;
        assert!(!status.can_force_stop(deadline, deadline));
    }

    #[test]
    fn force_stop_deadline_clears_on_start_completion_and_cancellation() {
        for operation in [
            Model::<StatusInfo>::start,
            Model::<StatusInfo>::started,
            Model::<StatusInfo>::stopped,
            Model::<StatusInfo>::restart,
            Model::<StatusInfo>::init,
        ] {
            let mut status = Model::new(&StatusInfo {
                force_stop_at: Some(Utc::now()),
                ..Default::default()
            })
            .unwrap();
            operation(&mut status).unwrap();
            assert!(status.de().unwrap().force_stop_at.is_none());
        }
    }

    #[test]
    fn force_stop_deadline_preserves_retries_and_clears_canceled_stops() {
        let now = Utc::now();
        let delay = std::time::Duration::from_secs(30);
        let mut status = StatusInfo {
            started: Some(now),
            ..Default::default()
        };
        status.sync_force_stop_deadline(now, delay);
        let deadline = status.force_stop_at.unwrap();
        assert_eq!(deadline, now + chrono::TimeDelta::seconds(30));
        status.sync_force_stop_deadline(now + chrono::TimeDelta::seconds(10), delay);
        assert_eq!(status.force_stop_at, Some(deadline));
        status.desired = DesiredStatus::Running;
        status.sync_force_stop_deadline(now, delay);
        assert_eq!(status.force_stop_at, None);
        status.desired = DesiredStatus::Stopped;
        status.sync_force_stop_deadline(now + chrono::TimeDelta::seconds(20), delay);
        assert_ne!(status.force_stop_at, Some(deadline));
        status.started = None;
        status.sync_force_stop_deadline(now, delay);
        assert_eq!(status.force_stop_at, None);
    }

    #[test]
    fn legacy_status_defaults_force_stop_deadline() {
        let status: StatusInfo = serde_json::from_value(serde_json::json!({
            "health": {}, "error": null, "started": null, "desired": {"main": "stopped"}
        }))
        .unwrap();
        assert!(status.force_stop_at.is_none());
    }

    #[test]
    fn restart_during_start_survives_completion() {
        for restart_in_progress in [false, true] {
            for requests in [1, 2, 3] {
                let mut status = Model::new(&StatusInfo::default()).unwrap();
                status.start().unwrap();
                if restart_in_progress {
                    status.started().unwrap();
                    status.restart().unwrap();
                    status.stopped().unwrap();
                }

                for _ in 0..requests {
                    status.restart().unwrap();
                }
                assert_eq!(
                    status.de().unwrap().desired,
                    DesiredStatus::Restarting {
                        restart_again: true,
                    }
                );
                assert!(status.de().unwrap().started.is_none());

                status.started().unwrap();
                assert_eq!(
                    status.de().unwrap().desired,
                    DesiredStatus::Restarting {
                        restart_again: false,
                    }
                );
                assert!(status.de().unwrap().started.is_some());

                status.stopped().unwrap();
                status.started().unwrap();
                assert_eq!(status.de().unwrap().desired, DesiredStatus::Running);
                assert!(status.de().unwrap().started.is_some());
            }
        }
    }

    #[test]
    fn restart_after_start_needs_one_stop_start_cycle() {
        let mut status = Model::new(&StatusInfo::default()).unwrap();
        status.start().unwrap();
        status.started().unwrap();
        for _ in 0..3 {
            status.restart().unwrap();
            assert_eq!(
                status.de().unwrap().desired,
                DesiredStatus::Restarting {
                    restart_again: false,
                }
            );
        }
        status.stopped().unwrap();
        status.started().unwrap();
        assert_eq!(status.de().unwrap().desired, DesiredStatus::Running);
    }

    #[test]
    fn stop_cancels_restart_during_start() {
        let mut status = Model::new(&StatusInfo::default()).unwrap();
        status.start().unwrap();
        status.restart().unwrap();
        status.stop().unwrap();
        status.started().unwrap();
        assert_eq!(status.de().unwrap().desired, DesiredStatus::Stopped);
        status.stopped().unwrap();
        assert!(status.de().unwrap().started.is_none());
    }

    #[test]
    fn restart_preserves_stopped_backup_and_update_states() {
        for desired in [
            DesiredStatus::Stopped,
            DesiredStatus::BackingUp {
                on_complete: StartStop::Start,
            },
            DesiredStatus::BackingUp {
                on_complete: StartStop::Stop,
            },
            DesiredStatus::Updating {
                on_complete: StartStop::Start,
            },
            DesiredStatus::Updating {
                on_complete: StartStop::Stop,
            },
        ] {
            for started in [false, true] {
                assert_eq!(desired.restart(started), desired);
            }
        }
    }
}
