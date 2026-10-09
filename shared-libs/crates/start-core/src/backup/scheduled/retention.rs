use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;
use chrono_tz::Tz;
use color_eyre::eyre::eyre;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ServiceSnapshot, ServiceSnapshotId};
use crate::prelude::*;

const MAX_RETENTION_SECONDS: u64 = 10 * 366 * 24 * 60 * 60;
const MAX_PROJECTED_SNAPSHOTS: u64 = 10_000;
pub(super) const CAPACITY_MARGIN_PERCENT: u8 = 10;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetentionTier {
    /// Width of one local-time retention bucket.
    #[ts(type = "number")]
    pub interval_seconds: u64,
    /// How far back from the newest successful snapshot this tier covers.
    #[ts(type = "number")]
    pub coverage_seconds: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetentionPolicy {
    /// An empty tier list is the latest-only policy.
    pub tiers: Vec<RetentionTier>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetentionPreview {
    pub retained: BTreeSet<ServiceSnapshotId>,
    pub removed: Vec<ServiceSnapshot>,
    #[ts(type = "number")]
    pub estimated_reclaimed_bytes: u64,
}

impl RetentionPolicy {
    pub fn latest_only() -> Self {
        Self::default()
    }

    pub fn validate(&self) -> Result<(), Error> {
        let mut previous_interval = 0;
        let mut previous_coverage = 0;
        let mut projected = 1u64;
        for tier in &self.tiers {
            if tier.interval_seconds == 0
                || tier.coverage_seconds == 0
                || tier.coverage_seconds < tier.interval_seconds
                || tier.coverage_seconds > MAX_RETENTION_SECONDS
                || tier.interval_seconds <= previous_interval
                || tier.coverage_seconds <= previous_coverage
            {
                return Err(Error::new(
                    eyre!("{}", t!("backup.scheduled.invalid-retention-tiers")),
                    ErrorKind::InvalidRequest,
                ));
            }
            projected = projected
                .checked_add(tier.coverage_seconds.div_ceil(tier.interval_seconds))
                .ok_or_else(retention_overflow)?;
            if projected > MAX_PROJECTED_SNAPSHOTS {
                return Err(retention_overflow());
            }
            previous_interval = tier.interval_seconds;
            previous_coverage = tier.coverage_seconds;
        }
        Ok(())
    }

    pub fn maximum_projected_snapshot_count(&self) -> Result<u64, Error> {
        self.validate()?;
        Ok(1 + self
            .tiers
            .iter()
            .map(|tier| tier.coverage_seconds.div_ceil(tier.interval_seconds))
            .sum::<u64>())
    }

    pub fn retained_snapshot_ids(
        &self,
        snapshots: &[ServiceSnapshot],
        timezone: Tz,
    ) -> Result<BTreeSet<ServiceSnapshotId>, Error> {
        self.validate()?;
        let mut successful: Vec<_> = snapshots
            .iter()
            .filter(|snapshot| !snapshot.archived)
            .collect();
        successful.sort_by_key(|snapshot| snapshot.completed_at);
        let Some(newest) = successful.last() else {
            return Ok(BTreeSet::new());
        };

        let mut retained = BTreeSet::from([newest.id.clone()]);
        for tier in &self.tiers {
            let coverage = Duration::seconds(
                i64::try_from(tier.coverage_seconds).map_err(|_| retention_overflow())?,
            );
            let cutoff = newest.completed_at - coverage;
            let mut buckets: BTreeMap<i64, &ServiceSnapshot> = BTreeMap::new();
            for snapshot in successful
                .iter()
                .copied()
                .filter(|s| s.completed_at >= cutoff)
            {
                let local_seconds = snapshot
                    .completed_at
                    .with_timezone(&timezone)
                    .naive_local()
                    .and_utc()
                    .timestamp();
                let bucket = local_seconds.div_euclid(tier.interval_seconds as i64);
                buckets
                    .entry(bucket)
                    .and_modify(|current| {
                        if current.completed_at < snapshot.completed_at {
                            *current = snapshot;
                        }
                    })
                    .or_insert(snapshot);
            }
            retained.extend(buckets.into_values().map(|snapshot| snapshot.id.clone()));
        }
        Ok(retained)
    }

    pub fn preview(
        &self,
        snapshots: &[ServiceSnapshot],
        timezone: Tz,
    ) -> Result<RetentionPreview, Error> {
        let retained = self.retained_snapshot_ids(snapshots, timezone)?;
        let removed: Vec<_> = snapshots
            .iter()
            .filter(|snapshot| !snapshot.archived && !retained.contains(&snapshot.id))
            .cloned()
            .collect();
        let estimated_reclaimed_bytes = removed
            .iter()
            .map(|snapshot| snapshot.physical_size.unwrap_or(snapshot.logical_size))
            .sum();
        Ok(RetentionPreview {
            retained,
            removed,
            estimated_reclaimed_bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapacityEstimate {
    pub retained_snapshot_count: usize,
    #[ts(type = "number")]
    pub maximum_projected_snapshot_count: u64,
    #[ts(type = "number")]
    pub scheduled_retained_bytes: u64,
    #[ts(type = "number")]
    pub manual_checkpoint_bytes: u64,
    #[ts(type = "number")]
    pub archived_bytes: u64,
    #[ts(type = "number")]
    pub staging_headroom_bytes: u64,
    #[ts(type = "number | null")]
    pub last_changed_bytes: Option<u64>,
    #[ts(type = "number")]
    pub conservative_peak_bytes: u64,
}

impl CapacityEstimate {
    pub fn calculate(
        policy: &RetentionPolicy,
        retained: &[ServiceSnapshot],
        manual_checkpoint_bytes: u64,
        archived_bytes: u64,
        live_logical_bytes: u64,
        safety_margin_percent: u8,
    ) -> Result<Self, Error> {
        let maximum_projected_snapshot_count = policy.maximum_projected_snapshot_count()?;
        let measured_copy_bytes = retained
            .iter()
            .max_by_key(|snapshot| snapshot.completed_at)
            .map(|snapshot| snapshot.physical_size.unwrap_or(snapshot.logical_size))
            .unwrap_or(live_logical_bytes)
            .max(live_logical_bytes);
        let scheduled_retained_bytes = retained
            .iter()
            .map(|snapshot| snapshot.physical_size.unwrap_or(snapshot.logical_size))
            .sum();
        let staging_headroom_bytes = with_margin(measured_copy_bytes, safety_margin_percent)
            .ok_or_else(retention_overflow)?;
        let projected_scheduled = measured_copy_bytes
            .checked_mul(maximum_projected_snapshot_count)
            .ok_or_else(retention_overflow)?;
        let conservative_peak_bytes = projected_scheduled
            .checked_add(manual_checkpoint_bytes)
            .and_then(|value| value.checked_add(archived_bytes))
            .and_then(|value| value.checked_add(staging_headroom_bytes))
            .ok_or_else(retention_overflow)?;

        Ok(Self {
            retained_snapshot_count: retained.len(),
            maximum_projected_snapshot_count,
            scheduled_retained_bytes,
            manual_checkpoint_bytes,
            archived_bytes,
            staging_headroom_bytes,
            last_changed_bytes: retained
                .iter()
                .max_by_key(|snapshot| snapshot.completed_at)
                .and_then(|snapshot| snapshot.changed_bytes),
            conservative_peak_bytes,
        })
    }
}

pub(super) fn with_margin(value: u64, percent: u8) -> Option<u64> {
    value
        .checked_mul(100 + u64::from(percent))
        .and_then(|value| value.checked_add(99))
        .map(|value| value / 100)
}

fn retention_overflow() -> Error {
    Error::new(
        eyre!("{}", t!("backup.scheduled.unreasonable-retention")),
        ErrorKind::InvalidRequest,
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::backup::scheduled::{BackupJobId, BackupRunId, BackupSource};
    use crate::id::PackageId;

    fn snapshot(hour: u32, logical_size: u64) -> ServiceSnapshot {
        ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            package_id: "test-service".parse::<PackageId>().unwrap(),
            package_version: "1.0.0".into(),
            source: BackupSource::Scheduled,
            job_id: BackupJobId::new(),
            job_name: "Nightly".into(),
            run_id: BackupRunId::new(),
            completed_at: Utc.with_ymd_and_hms(2025, 1, 2, hour, 30, 0).unwrap(),
            logical_size,
            physical_size: None,
            changed_bytes: None,
            measured_at: Utc.with_ymd_and_hms(2025, 1, 2, hour, 30, 0).unwrap(),
            archived: false,
        }
    }

    #[test]
    fn latest_only_always_retains_newest() {
        let snapshots = vec![snapshot(1, 10), snapshot(2, 20), snapshot(3, 30)];
        let ids = RetentionPolicy::latest_only()
            .retained_snapshot_ids(&snapshots, chrono_tz::UTC)
            .unwrap();
        assert_eq!(ids, BTreeSet::from([snapshots[2].id.clone()]));
    }

    #[test]
    fn overlapping_tiers_share_physical_snapshots() {
        let snapshots = vec![
            snapshot(0, 10),
            snapshot(1, 10),
            snapshot(2, 10),
            snapshot(3, 10),
        ];
        let policy = RetentionPolicy {
            tiers: vec![
                RetentionTier {
                    interval_seconds: 60 * 60,
                    coverage_seconds: 4 * 60 * 60,
                },
                RetentionTier {
                    interval_seconds: 2 * 60 * 60,
                    coverage_seconds: 8 * 60 * 60,
                },
            ],
        };
        let ids = policy
            .retained_snapshot_ids(&snapshots, chrono_tz::UTC)
            .unwrap();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn preview_is_exact_and_preserves_archives() {
        let mut snapshots = vec![snapshot(1, 10), snapshot(2, 20), snapshot(3, 30)];
        snapshots[0].archived = true;
        let preview = RetentionPolicy::latest_only()
            .preview(&snapshots, chrono_tz::UTC)
            .unwrap();
        assert_eq!(preview.removed.len(), 1);
        assert_eq!(preview.removed[0].id, snapshots[1].id);
        assert_eq!(preview.estimated_reclaimed_bytes, 20);
    }

    #[test]
    fn rejects_contradictory_or_unreasonable_tiers() {
        assert!(
            RetentionPolicy {
                tiers: vec![RetentionTier {
                    interval_seconds: 0,
                    coverage_seconds: 10,
                }],
            }
            .validate()
            .is_err()
        );
        assert!(
            RetentionPolicy {
                tiers: vec![
                    RetentionTier {
                        interval_seconds: 3600,
                        coverage_seconds: 86400,
                    },
                    RetentionTier {
                        interval_seconds: 1800,
                        coverage_seconds: 172800,
                    },
                ],
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn capacity_uses_full_copies_and_staging_not_changed_bytes() {
        let mut latest = snapshot(3, 1000);
        latest.changed_bytes = Some(5);
        let estimate = CapacityEstimate::calculate(
            &RetentionPolicy::latest_only(),
            &[latest],
            500,
            250,
            1000,
            10,
        )
        .unwrap();
        assert_eq!(estimate.staging_headroom_bytes, 1100);
        assert_eq!(estimate.last_changed_bytes, Some(5));
        assert_eq!(estimate.conservative_peak_bytes, 2850);
    }

    #[test]
    fn capacity_accepts_large_safety_margins_without_wrapping() {
        let estimate =
            CapacityEstimate::calculate(&RetentionPolicy::latest_only(), &[], 0, 0, 1000, u8::MAX)
                .unwrap();
        assert_eq!(estimate.staging_headroom_bytes, 3550);
        assert_eq!(estimate.conservative_peak_bytes, 4550);
    }

    #[test]
    fn sparse_history_keeps_every_available_retention_bucket() {
        let hourly = RetentionPolicy {
            tiers: vec![RetentionTier {
                interval_seconds: 60 * 60,
                coverage_seconds: 24 * 60 * 60,
            }],
        };
        let snapshots = vec![snapshot(0, 10), snapshot(12, 20), snapshot(23, 30)];

        assert_eq!(
            hourly
                .retained_snapshot_ids(&snapshots, chrono_tz::UTC)
                .unwrap()
                .len(),
            snapshots.len(),
        );
    }
}
