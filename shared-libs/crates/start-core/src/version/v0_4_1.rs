use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_5};
use crate::context::RpcContext;
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_1: exver::Version = exver::Version::new([0, 4, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

// Includes earlier 0.4.0.5 migration revisions during direct upgrades.
impl VersionT for Version {
    type Previous = v0_4_0_5::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, input: Self::PreUpRes) -> Result<Value, Error> {
        v0_4_0_5::Version.up(db, input)
    }
    async fn post_up(self, ctx: &RpcContext, input: Value) -> Result<(), Error> {
        v0_4_0_5::Version.post_up(ctx, input).await
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}
