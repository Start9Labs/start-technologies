use std::collections::BTreeSet;
use std::time::Duration;

use tokio::process::Command;

use crate::db::model::Database;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::util::{Invoke, btrfs};
use crate::{MAIN_DATA, PACKAGE_DATA};

#[derive(Debug, PartialEq)]
struct Allocation {
    size: u64,
    unallocated: u64,
}

impl Allocation {
    fn parse(output: &str) -> Result<Self, Error> {
        let field = |name: &str| -> Result<u64, Error> {
            output
                .lines()
                .find_map(|line| line.trim().strip_prefix(name))
                .or_not_found(name)?
                .trim()
                .parse::<u64>()
                .with_kind(ErrorKind::ParseSysInfo)
        };
        Ok(Self {
            size: field("Device size:")?,
            unallocated: field("Device unallocated:")?,
        })
    }

    fn is_low(&self) -> bool {
        self.unallocated < (self.size / 20).min(10 << 30)
    }
}

#[derive(Default)]
pub(super) struct DiskHealth {
    warned: BTreeSet<&'static str>,
}

impl DiskHealth {
    pub(super) async fn check(&mut self, db: &TypedPatchDb<Database>) {
        for path in [MAIN_DATA, PACKAGE_DATA] {
            if let Err(error) = self.check_path(db, path).await {
                tracing::warn!(?error, path, "{}", t!("system.disk-health.check-failed"));
            }
        }
    }

    async fn check_path(
        &mut self,
        db: &TypedPatchDb<Database>,
        path: &'static str,
    ) -> Result<(), Error> {
        if !btrfs::is_btrfs(path).await {
            return Ok(());
        }
        let output = Command::new("btrfs")
            .args(["filesystem", "usage", "--raw", path])
            .env("LC_ALL", "C")
            .timeout(Some(Duration::from_secs(30)))
            .invoke(ErrorKind::Filesystem)
            .await?;
        let allocation =
            Allocation::parse(std::str::from_utf8(&output).with_kind(ErrorKind::ParseSysInfo)?)?;
        self.update(db, path, allocation).await
    }

    async fn update(
        &mut self,
        db: &TypedPatchDb<Database>,
        path: &'static str,
        allocation: Allocation,
    ) -> Result<(), Error> {
        if !allocation.is_low() {
            self.warned.remove(path);
        } else if !self.warned.contains(path) {
            db.mutate(|db| {
                notify(
                    db,
                    None,
                    NotificationLevel::Warning,
                    t!("system.disk-health.low-unallocated-title").into(),
                    t!("system.disk-health.low-unallocated-message", path = path).into(),
                    (),
                )
            })
            .await
            .result?;
            self.warned.insert(path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_usage_without_confusing_free_space_with_unallocated() {
        let allocation = Allocation::parse(
            "Overall:\n    Device size: 1253443256320\n    Device allocated: 1253442207744\n    Device unallocated: 1048576\n    Used: 960000000000\n    Free (estimated): 293443256320 (min: 293443256320)\n\nData,single: Size:1250751897600, Used:955630223360 (76.40%)\n",
        )
        .unwrap();
        assert_eq!(
            allocation,
            Allocation {
                size: 1253443256320,
                unallocated: 1 << 20
            }
        );
        assert!(allocation.is_low());
    }

    #[test]
    fn rejects_incomplete_or_non_raw_usage() {
        for output in [
            "Device size: 100\n",
            "Device unallocated: 10\n",
            "Device size: 1.14TiB\nDevice unallocated: 1.00MiB\n",
        ] {
            assert!(Allocation::parse(output).is_err());
        }
    }

    #[test]
    fn reserve_scales_with_small_disks_and_is_capped_for_large_disks() {
        for (size, threshold) in [(8 << 30, 429496729), (1 << 40, 10 << 30)] {
            assert!(
                Allocation {
                    size,
                    unallocated: threshold - 1
                }
                .is_low()
            );
            assert!(
                !Allocation {
                    size,
                    unallocated: threshold
                }
                .is_low()
            );
        }
    }

    #[tokio::test]
    async fn warns_once_per_filesystem_until_recovery() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let db =
            TypedPatchDb::<Database>::load_unchecked(PatchDb::open(temp.path().join("db")).await?);
        db.mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_unread_notification_count_mut()
                .ser(&0)?;
            db.as_private_mut()
                .as_notifications_mut()
                .ser(&crate::notifications::Notifications::new())
        })
        .await
        .result?;
        let mut health = DiskHealth::default();
        health
            .update(
                &db,
                MAIN_DATA,
                Allocation {
                    size: 1 << 40,
                    unallocated: 10 << 30,
                },
            )
            .await?;
        assert!(
            db.peek()
                .await
                .as_private()
                .as_notifications()
                .keys()?
                .is_empty()
        );
        for path in [MAIN_DATA, MAIN_DATA, PACKAGE_DATA, PACKAGE_DATA] {
            health
                .update(
                    &db,
                    path,
                    Allocation {
                        size: 1 << 40,
                        unallocated: 1 << 20,
                    },
                )
                .await?;
        }
        assert_eq!(
            db.peek()
                .await
                .as_private()
                .as_notifications()
                .keys()?
                .len(),
            2
        );
        for (index, path) in [MAIN_DATA, PACKAGE_DATA].into_iter().enumerate() {
            let notification = db
                .peek()
                .await
                .as_private()
                .as_notifications()
                .as_idx(&(index as u32))
                .unwrap()
                .de()?;
            assert_eq!(notification.level, NotificationLevel::Warning);
            assert!(notification.package_id.is_none());
            assert!(!notification.seen);
            assert!(notification.message.contains(path));
        }
        health
            .update(
                &db,
                MAIN_DATA,
                Allocation {
                    size: 1 << 40,
                    unallocated: 10 << 30,
                },
            )
            .await?;
        health
            .update(
                &db,
                MAIN_DATA,
                Allocation {
                    size: 1 << 40,
                    unallocated: 0,
                },
            )
            .await?;
        assert_eq!(
            db.peek()
                .await
                .as_private()
                .as_notifications()
                .keys()?
                .len(),
            3
        );
        assert_eq!(
            db.peek()
                .await
                .as_public()
                .as_server_info()
                .as_unread_notification_count()
                .de()?,
            3
        );
        Ok(())
    }
}
