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
}
impl StatusInfo {
    pub fn stop(&mut self) {
        self.desired = self.desired.stop();
        self.health.clear();
    }
}
impl Model<StatusInfo> {
    pub fn start(&mut self) -> Result<(), Error> {
        self.as_desired_mut().map_mutate(|s| Ok(s.start()))?;
        Ok(())
    }
    pub fn started(&mut self) -> Result<(), Error> {
        self.as_started_mut()
            .map_mutate(|s| Ok(Some(s.unwrap_or_else(|| Utc::now()))))?;
        self.as_desired_mut().map_mutate(|s| {
            Ok(match s {
                DesiredStatus::Restarting {
                    restart_again: true,
                } => {
                    // Clear the flag but stay Restarting so actor will stop→start again
                    DesiredStatus::Restarting {
                        restart_again: false,
                    }
                }
                DesiredStatus::Restarting {
                    restart_again: false,
                } => DesiredStatus::Running,
                a => a,
            })
        })?;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        self.as_desired_mut().map_mutate(|s| Ok(s.stop()))?;
        self.as_health_mut().ser(&Default::default())?;
        Ok(())
    }
    pub fn stopped(&mut self) -> Result<(), Error> {
        self.as_started_mut().ser(&None)?;
        self.as_health_mut().ser(&Default::default())?;
        Ok(())
    }
    pub fn restart(&mut self) -> Result<(), Error> {
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

    pub fn restart(&self, started: bool) -> Self {
        match self {
            Self::Running => Self::Restarting {
                restart_again: false,
            },
            Self::Restarting { .. } if !started => Self::Restarting {
                restart_again: true,
            },
            x => *x,
        }
    }
}
