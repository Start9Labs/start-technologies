use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_1: exver::Version = exver::Version::new([0, 4, 0, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}
