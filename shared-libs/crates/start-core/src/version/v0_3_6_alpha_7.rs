use exver::{PreReleaseSegment, VersionRange};
use imbl_value::json;
use tokio::process::Command;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_3_6_alpha_6};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;

lazy_static::lazy_static! {
    static ref V0_3_6_alpha_7: exver::Version = exver::Version::new(
        [0, 3, 6],
        [PreReleaseSegment::String("alpha".into()), 7.into()]
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_3_6_alpha_6::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_3_6_alpha_7.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let server_info = db["public"]["serverInfo"]
            .as_object_mut()
            .or_not_found("public.serverInfo")?;
        server_info.insert("ram".into(), json!(0));
        server_info.insert("devices".into(), json!([]));
        let package_data = db["public"]["packageData"]
            .as_object_mut()
            .or_not_found("public.packageData")?;
        for (_, pde) in package_data.iter_mut() {
            if let Some(manifest) = pde["stateInfo"].get_mut("manifest") {
                manifest["hardwareRequirements"]["device"] = json!([]);
            }
        }
        Ok(Value::Null)
    }
    async fn post_up(self, ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        Command::new("systemd-firstboot")
            .arg("--root=/media/startos/config/overlay/")
            .arg(
                ctx.account
                    .peek(|a| format!("--hostname={}", a.hostname.as_ref())),
            )
            .invoke(ErrorKind::ParseSysInfo)
            .await?;
        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}
