use exver::VersionRange;
use tokio::process::Command;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_4};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;

lazy_static::lazy_static! {
    static ref V0_4_0_5: exver::Version = exver::Version::new([0, 4, 0, 5], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_4::Version;
    type PreUpRes = ();

    fn migration_revision(self) -> usize {
        1
    }
    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_5.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    async fn post_up(self, _ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        start_initial_balance(Command::new("systemctl")).await;
        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

async fn start_initial_balance(mut systemctl: Command) {
    if let Err(error) = systemctl
        .args(["start", "--no-block", "btrfs-balance.service"])
        .invoke(ErrorKind::DiskManagement)
        .await
    {
        tracing::warn!(?error, "{}", t!("migration.initial-balance-failed"));
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn initial_balance_advances_the_published_migration_revision() {
        assert_eq!(Version.migration_revision(), 1);
    }

    #[tokio::test]
    async fn initial_balance_queues_the_existing_service_without_failing_startup()
    -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let systemctl = temp.path().join("systemctl");
        let calls = temp.path().join("calls");
        std::fs::write(
            &systemctl,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CALL_LOG\"\nexit \"$EXIT_STATUS\"\n",
        )?;
        std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755))?;
        for status in [0, 7] {
            let mut command = Command::new(&systemctl);
            command
                .env("CALL_LOG", &calls)
                .env("EXIT_STATUS", status.to_string());
            let () = start_initial_balance(command).await;
            assert_eq!(
                std::fs::read_to_string(&calls)?,
                "start\n--no-block\nbtrfs-balance.service\n"
            );
        }
        Ok(())
    }
}
