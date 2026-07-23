use std::any::Any;
use std::cmp::Ordering;
use std::panic::{RefUnwindSafe, UnwindSafe};

use color_eyre::eyre::eyre;
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use imbl_value::{InternedString, to_value};
use patch_db::json_ptr::ROOT;

use crate::Error;
use crate::context::RpcContext;
use crate::db::model::Database;
use crate::prelude::*;
use crate::progress::PhaseProgressTrackerHandle;

mod v0_3_5;
mod v0_3_5_1;
mod v0_3_5_2;
mod v0_3_6_alpha_0;
mod v0_3_6_alpha_1;
mod v0_3_6_alpha_2;
mod v0_3_6_alpha_3;
mod v0_3_6_alpha_4;
mod v0_3_6_alpha_5;
mod v0_3_6_alpha_6;
mod v0_3_6_alpha_7;
mod v0_3_6_alpha_8;
mod v0_3_6_alpha_9;

mod v0_3_6_alpha_10;
mod v0_3_6_alpha_11;
mod v0_3_6_alpha_12;
mod v0_3_6_alpha_13;
mod v0_3_6_alpha_14;
mod v0_3_6_alpha_15;
mod v0_3_6_alpha_16;
mod v0_3_6_alpha_17;
mod v0_3_6_alpha_18;

mod v0_4_0;
mod v0_4_0_alpha_0;
mod v0_4_0_alpha_1;
mod v0_4_0_alpha_2;
mod v0_4_0_alpha_3;
mod v0_4_0_alpha_4;
mod v0_4_0_alpha_5;
mod v0_4_0_alpha_6;
mod v0_4_0_alpha_7;
mod v0_4_0_alpha_8;
mod v0_4_0_alpha_9;

mod v0_4_0_alpha_10;
mod v0_4_0_alpha_11;
mod v0_4_0_alpha_12;
mod v0_4_0_alpha_13;
mod v0_4_0_alpha_14;
mod v0_4_0_alpha_15;
mod v0_4_0_alpha_16;
mod v0_4_0_alpha_17;
mod v0_4_0_alpha_18;
mod v0_4_0_alpha_19;
mod v0_4_0_alpha_20;
mod v0_4_0_alpha_21;
mod v0_4_0_alpha_22;
mod v0_4_0_alpha_23;
mod v0_4_0_beta_0;
mod v0_4_0_beta_1;
mod v0_4_0_beta_10;
mod v0_4_0_beta_2;
mod v0_4_0_beta_3;
mod v0_4_0_beta_4;
mod v0_4_0_beta_5;
mod v0_4_0_beta_6;
mod v0_4_0_beta_7;
mod v0_4_0_beta_8;
mod v0_4_0_beta_9;

pub type Current = v0_4_0::Version; // VERSION_BUMP

impl Current {
    #[instrument(skip(self, db))]
    pub async fn pre_init(self, db: &PatchDb) -> Result<(), Error> {
        let from = from_value::<Version>(
            version_accessor(&mut db.dump(&ROOT).await.value)
                .or_not_found("`version` in db")?
                .clone(),
        )?
        .as_version_t()?;
        match from.semver().cmp(&self.semver()) {
            Ordering::Greater => {
                db.apply_function(|mut db| {
                    rollback_to_unchecked(&from, &self, &mut db)?;
                    Ok::<_, Error>((db, ()))
                })
                .await
                .result?;
            }
            Ordering::Less => {
                let pre_ups = PreUps::load(&from, &self).await?;
                db.apply_function(|mut db| {
                    migrate_from_unchecked(&from, &self, pre_ups, &mut db)?;
                    Ok::<_, Error>((to_value(&from_value::<Database>(db.clone())?)?, ()))
                })
                .await
                .result?;
            }
            Ordering::Equal => {
                db.apply_function(|db| {
                    Ok::<_, Error>((to_value(&from_value::<Database>(db.clone())?)?, ()))
                })
                .await
                .result?;
            }
        }
        Ok(())
    }
}

pub async fn post_init(
    ctx: &RpcContext,
    mut progress: PhaseProgressTrackerHandle,
) -> Result<(), Error> {
    let mut peek = ctx.db.peek().await;
    let todos = peek
        .as_public()
        .as_server_info()
        .as_post_init_migration_todos()
        .de()?;
    progress.start();
    if !todos.is_empty() {
        while let Some((version, input)) = {
            peek = ctx.db.peek().await;
            peek.as_public()
                .as_server_info()
                .as_post_init_migration_todos()
                .de()?
                .pop_first()
                .map(|(version, input)| {
                    Ok::<_, Error>((Version::from_exver_version(version).as_version_t()?, input))
                })
                .transpose()?
        } {
            version.0.post_up(ctx, input).await?;
            ctx.db
                .mutate(|db| {
                    db.as_public_mut()
                        .as_server_info_mut()
                        .as_post_init_migration_todos_mut()
                        .mutate(|m| Ok(m.remove(&version.0.semver())))
                })
                .await
                .result?;
        }
    }
    progress.complete();
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(untagged)]
#[allow(non_camel_case_types)]
enum Version {
    LT0_3_5(LTWrapper<v0_3_5::Version>),
    V0_3_5(Wrapper<v0_3_5::Version>),
    V0_3_5_1(Wrapper<v0_3_5_1::Version>),
    V0_3_5_2(Wrapper<v0_3_5_2::Version>),
    V0_3_6_alpha_0(Wrapper<v0_3_6_alpha_0::Version>),
    V0_3_6_alpha_1(Wrapper<v0_3_6_alpha_1::Version>),
    V0_3_6_alpha_2(Wrapper<v0_3_6_alpha_2::Version>),
    V0_3_6_alpha_3(Wrapper<v0_3_6_alpha_3::Version>),
    V0_3_6_alpha_4(Wrapper<v0_3_6_alpha_4::Version>),
    V0_3_6_alpha_5(Wrapper<v0_3_6_alpha_5::Version>),
    V0_3_6_alpha_6(Wrapper<v0_3_6_alpha_6::Version>),
    V0_3_6_alpha_7(Wrapper<v0_3_6_alpha_7::Version>),
    V0_3_6_alpha_8(Wrapper<v0_3_6_alpha_8::Version>),
    V0_3_6_alpha_9(Wrapper<v0_3_6_alpha_9::Version>),
    V0_3_6_alpha_10(Wrapper<v0_3_6_alpha_10::Version>),
    V0_3_6_alpha_11(Wrapper<v0_3_6_alpha_11::Version>),
    V0_3_6_alpha_12(Wrapper<v0_3_6_alpha_12::Version>),
    V0_3_6_alpha_13(Wrapper<v0_3_6_alpha_13::Version>),
    V0_3_6_alpha_14(Wrapper<v0_3_6_alpha_14::Version>),
    V0_3_6_alpha_15(Wrapper<v0_3_6_alpha_15::Version>),
    V0_3_6_alpha_16(Wrapper<v0_3_6_alpha_16::Version>),
    V0_3_6_alpha_17(Wrapper<v0_3_6_alpha_17::Version>),
    V0_3_6_alpha_18(Wrapper<v0_3_6_alpha_18::Version>),
    V0_4_0_alpha_0(Wrapper<v0_4_0_alpha_0::Version>),
    V0_4_0_alpha_1(Wrapper<v0_4_0_alpha_1::Version>),
    V0_4_0_alpha_2(Wrapper<v0_4_0_alpha_2::Version>),
    V0_4_0_alpha_3(Wrapper<v0_4_0_alpha_3::Version>),
    V0_4_0_alpha_4(Wrapper<v0_4_0_alpha_4::Version>),
    V0_4_0_alpha_5(Wrapper<v0_4_0_alpha_5::Version>),
    V0_4_0_alpha_6(Wrapper<v0_4_0_alpha_6::Version>),
    V0_4_0_alpha_7(Wrapper<v0_4_0_alpha_7::Version>),
    V0_4_0_alpha_8(Wrapper<v0_4_0_alpha_8::Version>),
    V0_4_0_alpha_9(Wrapper<v0_4_0_alpha_9::Version>),
    V0_4_0_alpha_10(Wrapper<v0_4_0_alpha_10::Version>),
    V0_4_0_alpha_11(Wrapper<v0_4_0_alpha_11::Version>),
    V0_4_0_alpha_12(Wrapper<v0_4_0_alpha_12::Version>),
    V0_4_0_alpha_13(Wrapper<v0_4_0_alpha_13::Version>),
    V0_4_0_alpha_14(Wrapper<v0_4_0_alpha_14::Version>),
    V0_4_0_alpha_15(Wrapper<v0_4_0_alpha_15::Version>),
    V0_4_0_alpha_16(Wrapper<v0_4_0_alpha_16::Version>),
    V0_4_0_alpha_17(Wrapper<v0_4_0_alpha_17::Version>),
    V0_4_0_alpha_18(Wrapper<v0_4_0_alpha_18::Version>),
    V0_4_0_alpha_19(Wrapper<v0_4_0_alpha_19::Version>),
    V0_4_0_alpha_20(Wrapper<v0_4_0_alpha_20::Version>),
    V0_4_0_alpha_21(Wrapper<v0_4_0_alpha_21::Version>),
    V0_4_0_alpha_22(Wrapper<v0_4_0_alpha_22::Version>),
    V0_4_0_alpha_23(Wrapper<v0_4_0_alpha_23::Version>),
    V0_4_0_beta_0(Wrapper<v0_4_0_beta_0::Version>),
    V0_4_0_beta_1(Wrapper<v0_4_0_beta_1::Version>),
    V0_4_0_beta_2(Wrapper<v0_4_0_beta_2::Version>),
    V0_4_0_beta_3(Wrapper<v0_4_0_beta_3::Version>),
    V0_4_0_beta_4(Wrapper<v0_4_0_beta_4::Version>),
    V0_4_0_beta_5(Wrapper<v0_4_0_beta_5::Version>),
    V0_4_0_beta_6(Wrapper<v0_4_0_beta_6::Version>),
    V0_4_0_beta_7(Wrapper<v0_4_0_beta_7::Version>),
    V0_4_0_beta_8(Wrapper<v0_4_0_beta_8::Version>),
    V0_4_0_beta_9(Wrapper<v0_4_0_beta_9::Version>),
    V0_4_0_beta_10(Wrapper<v0_4_0_beta_10::Version>),
    V0_4_0(Wrapper<v0_4_0::Version>), // VERSION_BUMP
    Other(exver::Version),
}

impl Version {
    fn from_exver_version(version: exver::Version) -> Self {
        serde_json::to_value(version.clone())
            .and_then(serde_json::from_value)
            .unwrap_or_else(|_e| {
                tracing::warn!("Can't deserialize: {:?} and falling back to other", version);
                Version::Other(version)
            })
    }
    fn as_version_t(&self) -> Result<DynVersion, Error> {
        Ok(match self {
            Self::LT0_3_5(_) => {
                return Err(Error::new(
                    eyre!("cannot migrate from versions before 0.3.5"),
                    ErrorKind::MigrationFailed,
                ));
            }
            Self::V0_3_5(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_5_1(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_5_2(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_0(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_1(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_2(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_3(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_4(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_5(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_6(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_7(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_8(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_9(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_10(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_11(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_12(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_13(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_14(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_15(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_16(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_17(v) => DynVersion(Box::new(v.0)),
            Self::V0_3_6_alpha_18(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_0(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_1(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_2(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_3(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_4(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_5(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_6(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_7(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_8(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_9(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_10(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_11(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_12(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_13(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_14(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_15(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_16(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_17(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_18(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_19(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_20(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_21(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_22(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_23(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_0(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_1(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_2(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_3(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_4(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_5(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_6(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_7(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_8(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_9(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_beta_10(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0(v) => DynVersion(Box::new(v.0)), // VERSION_BUMP
            Self::Other(v) => {
                return Err(Error::new(
                    eyre!("unknown version {v}"),
                    ErrorKind::MigrationFailed,
                ));
            }
        })
    }
    #[cfg(test)]
    fn as_exver(&self) -> exver::Version {
        match self {
            Version::LT0_3_5(LTWrapper(_, x)) => x.clone(),
            Version::V0_3_5(Wrapper(x)) => x.semver(),
            Version::V0_3_5_1(Wrapper(x)) => x.semver(),
            Version::V0_3_5_2(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_0(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_1(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_2(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_3(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_4(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_5(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_6(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_7(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_8(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_9(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_10(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_11(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_12(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_13(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_14(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_15(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_16(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_17(Wrapper(x)) => x.semver(),
            Version::V0_3_6_alpha_18(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_0(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_1(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_2(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_3(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_4(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_5(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_6(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_7(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_8(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_9(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_10(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_11(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_12(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_13(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_14(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_15(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_16(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_17(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_18(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_19(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_20(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_21(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_22(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_23(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_0(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_1(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_2(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_3(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_4(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_5(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_6(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_7(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_8(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_9(Wrapper(x)) => x.semver(),
            Version::V0_4_0_beta_10(Wrapper(x)) => x.semver(),
            Version::V0_4_0(Wrapper(x)) => x.semver(), // VERSION_BUMP
            Version::Other(x) => x.clone(),
        }
    }
}

#[instrument(skip_all)]
fn version_accessor(db: &mut Value) -> Option<&mut Value> {
    if db.get("public").is_some() {
        db.get_mut("public")?
            .get_mut("serverInfo")?
            .get_mut("version")
    } else {
        db.get_mut("server-info")?.get_mut("version")
    }
}

#[instrument(skip_all)]
fn version_compat_accessor(db: &mut Value) -> Option<&mut Value> {
    if db.get("public").is_some() {
        let server_info = db.get_mut("public")?.get_mut("serverInfo")?;
        if server_info.get("packageVersionCompat").is_some() {
            server_info.get_mut("packageVersionCompat")
        } else {
            if let Some(prev) = server_info.get("eosVersionCompat").cloned() {
                server_info
                    .as_object_mut()?
                    .insert("packageVersionCompat".into(), prev);
            } else if let Some(prev) = server_info.get("versionCompat").cloned() {
                server_info
                    .as_object_mut()?
                    .insert("packageVersionCompat".into(), prev);
            }
            server_info.get_mut("packageVersionCompat")
        }
    } else {
        db.get_mut("server-info")?.get_mut("eos-version-compat")
    }
}

#[instrument(skip_all)]
fn post_init_migration_todos_accessor(db: &mut Value) -> Option<&mut Value> {
    let server_info = if db.get("public").is_some() {
        db.get_mut("public")?.get_mut("serverInfo")?
    } else {
        db.get_mut("server-info")?
    };
    if server_info.get("postInitMigrationTodos").is_none() {
        server_info.as_object_mut()?.insert(
            "postInitMigrationTodos".into(),
            Value::Object(Default::default()),
        );
    }
    server_info.get_mut("postInitMigrationTodos")
}

struct PreUps {
    prev: Option<Box<PreUps>>,
    value: Box<dyn Any + UnwindSafe + Send + 'static>,
}
impl PreUps {
    #[instrument(skip(from, to))]
    fn load<'a, VFrom: DynVersionT + ?Sized, VTo: DynVersionT + ?Sized>(
        from: &'a VFrom,
        to: &'a VTo,
    ) -> BoxFuture<'a, Result<Self, Error>> {
        async {
            let previous = to.previous();
            let prev = match from.semver().cmp(&previous.semver()) {
                Ordering::Less => Some(Box::new(PreUps::load(from, &previous).await?)),
                Ordering::Greater => {
                    return Err(Error::new(
                        eyre!(
                            "NO PATH FROM {}, THIS IS LIKELY A MISTAKE IN THE VERSION DEFINITION",
                            from.semver()
                        ),
                        crate::ErrorKind::MigrationFailed,
                    ));
                }
                Ordering::Equal => None,
            };
            Ok(Self {
                prev,
                value: to.pre_up().await?,
            })
        }
        .boxed()
    }
}

fn migrate_from_unchecked<VFrom: DynVersionT + ?Sized, VTo: DynVersionT + ?Sized>(
    from: &VFrom,
    to: &VTo,
    pre_ups: PreUps,
    db: &mut Value,
) -> Result<(), Error> {
    let previous = to.previous();
    match pre_ups.prev {
        Some(prev) if from.semver() < previous.semver() => {
            migrate_from_unchecked(from, &previous, *prev, db)?
        }
        _ if from.semver() > previous.semver() => {
            return Err(Error::new(
                eyre!(
                    "NO PATH FROM {}, THIS IS LIKELY A MISTAKE IN THE VERSION DEFINITION",
                    from.semver()
                ),
                crate::ErrorKind::MigrationFailed,
            ));
        }
        _ => (),
    };
    let res = to.up(db, pre_ups.value)?;
    to.commit(db, res)?;
    Ok(())
}

fn rollback_to_unchecked<VFrom: DynVersionT + ?Sized, VTo: DynVersionT + ?Sized>(
    from: &VFrom,
    to: &VTo,
    db: &mut Value,
) -> Result<(), Error> {
    let previous = from.previous();
    from.down(db)?;
    previous.commit(db, Value::Null)?;
    if to.semver() < previous.semver() {
        rollback_to_unchecked(&previous, to, db)?
    } else if to.semver() > previous.semver() {
        return Err(Error::new(
            eyre!(
                "NO PATH TO {}, THIS IS LIKELY A MISTAKE IN THE VERSION DEFINITION",
                to.semver()
            ),
            crate::ErrorKind::MigrationFailed,
        ));
    }
    Ok(())
}

#[allow(unused_variables)]
pub trait VersionT
where
    Self: Default + Copy + Sized + RefUnwindSafe + Send + Sync + 'static,
{
    type Previous: VersionT;
    type PreUpRes: Send + UnwindSafe;
    fn semver(self) -> exver::Version;
    fn compat(self) -> &'static exver::VersionRange;
    /// MUST be idempotent, and is run before *all* db migrations
    fn pre_up(self) -> impl Future<Output = Result<Self::PreUpRes, Error>> + Send + 'static;
    fn up(self, db: &mut Value, input: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    /// MUST be idempotent, and is run after *all* db migrations
    fn post_up<'a>(
        self,
        ctx: &'a RpcContext,
        input: Value,
    ) -> impl Future<Output = Result<(), Error>> + Send + 'a {
        async { Ok(()) }
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        Err(Error::new(
            eyre!("downgrades prohibited"),
            ErrorKind::InvalidRequest,
        ))
    }
    #[instrument(skip_all)]
    fn commit(self, db: &mut Value, res: Value) -> Result<(), Error> {
        *version_accessor(db).or_not_found("`public.serverInfo.version` in db")? =
            to_value(&self.semver())?;
        *version_compat_accessor(db).or_not_found("`public.serverInfo.versionCompat` in db")? =
            to_value(self.compat())?;
        if let Some(arr) = post_init_migration_todos_accessor(db)
            .or_not_found("`public.serverInfo.postInitMigrationTodos` in db")?
            .as_array_mut()
        {
            arr.push_back(to_value(&self.semver())?);
        } else if let Some(obj) = post_init_migration_todos_accessor(db)
            .or_not_found("`public.serverInfo.postInitMigrationTodos` in db")?
            .as_object_mut()
        {
            obj.insert(InternedString::from_display(&self.semver()), res);
        } else {
            return Err(Error::new(
                eyre!("postInitMigrationTodos is not an array or object"),
                ErrorKind::Database,
            ));
        }
        Ok(())
    }
}

struct DynVersion(Box<dyn DynVersionT>);
unsafe impl Send for DynVersion {}

trait DynVersionT: RefUnwindSafe + Send + Sync {
    fn previous(&self) -> DynVersion;
    fn semver(&self) -> exver::Version;
    // part of the DynVersionT surface; kept intentionally though not dynamically dispatched
    #[allow(dead_code)]
    fn compat(&self) -> &'static exver::VersionRange;
    fn pre_up(&self) -> BoxFuture<'static, Result<Box<dyn Any + UnwindSafe + Send>, Error>>;
    fn up(&self, db: &mut Value, input: Box<dyn Any + Send>) -> Result<Value, Error>;
    fn post_up<'a>(&self, ctx: &'a RpcContext, input: Value) -> BoxFuture<'a, Result<(), Error>>;
    fn down(&self, db: &mut Value) -> Result<(), Error>;
    fn commit(&self, db: &mut Value, res: Value) -> Result<(), Error>;
}
impl<T> DynVersionT for T
where
    T: VersionT,
{
    fn previous(&self) -> DynVersion {
        DynVersion(Box::new(<Self as VersionT>::Previous::default()))
    }
    fn semver(&self) -> exver::Version {
        VersionT::semver(*self)
    }
    fn compat(&self) -> &'static exver::VersionRange {
        VersionT::compat(*self)
    }
    fn pre_up(&self) -> BoxFuture<'static, Result<Box<dyn Any + UnwindSafe + Send>, Error>> {
        let v = *self;
        async move { Ok(Box::new(VersionT::pre_up(v).await?) as Box<dyn Any + UnwindSafe + Send>) }
            .boxed()
    }
    fn up(&self, db: &mut Value, input: Box<dyn Any + Send>) -> Result<Value, Error> {
        VersionT::up(
            *self,
            db,
            *input.downcast().map_err(|_| {
                Error::new(
                    eyre!("pre_up returned unexpected type"),
                    ErrorKind::Incoherent,
                )
            })?,
        )
    }
    fn post_up<'a>(&self, ctx: &'a RpcContext, input: Value) -> BoxFuture<'a, Result<(), Error>> {
        VersionT::post_up(*self, ctx, input).boxed()
    }
    fn down(&self, db: &mut Value) -> Result<(), Error> {
        VersionT::down(*self, db)
    }
    fn commit(&self, db: &mut Value, res: Value) -> Result<(), Error> {
        VersionT::commit(*self, db, res)
    }
}
impl DynVersionT for DynVersion {
    fn previous(&self) -> DynVersion {
        self.0.previous()
    }
    fn semver(&self) -> exver::Version {
        self.0.semver()
    }
    fn compat(&self) -> &'static exver::VersionRange {
        self.0.compat()
    }
    fn pre_up(&self) -> BoxFuture<'static, Result<Box<dyn Any + UnwindSafe + Send>, Error>> {
        self.0.pre_up()
    }
    fn up(&self, db: &mut Value, input: Box<dyn Any + Send>) -> Result<Value, Error> {
        self.0.up(db, input)
    }
    fn post_up<'a>(&self, ctx: &'a RpcContext, input: Value) -> BoxFuture<'a, Result<(), Error>> {
        self.0.post_up(ctx, input)
    }
    fn down(&self, db: &mut Value) -> Result<(), Error> {
        self.0.down(db)
    }
    fn commit(&self, db: &mut Value, res: Value) -> Result<(), Error> {
        self.0.commit(db, res)
    }
}

#[derive(Debug, Clone)]
struct LTWrapper<T>(T, #[allow(dead_code)] exver::Version);
impl<T> serde::Serialize for LTWrapper<T>
where
    T: VersionT,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.semver().serialize(serializer)
    }
}
impl<'de, T> serde::Deserialize<'de> for LTWrapper<T>
where
    T: VersionT,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = exver::Version::deserialize(deserializer)?;
        let version = T::default();
        if v < version.semver() {
            Ok(Self(version, v))
        } else {
            Err(serde::de::Error::custom("Mismatched Version"))
        }
    }
}

#[derive(Debug, Clone)]
struct Wrapper<T>(T);
impl<T> serde::Serialize for Wrapper<T>
where
    T: VersionT,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.semver().serialize(serializer)
    }
}
impl<'de, T> serde::Deserialize<'de> for Wrapper<T>
where
    T: VersionT,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = exver::Version::deserialize(deserializer)?;
        let version = T::default();
        if v == version.semver() {
            Ok(Wrapper(version))
        } else {
            Err(serde::de::Error::custom("Mismatched Version"))
        }
    }
}

pub const COMMIT_HASH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../build/env/GIT_HASH.txt"
));

pub const ENVIRONMENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../build/env/ENVIRONMENT.txt"
));

pub fn git_info() -> Result<InternedString, Error> {
    Ok(InternedString::intern(COMMIT_HASH))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn em_version() -> impl Strategy<Value = exver::Version> {
        any::<(usize, usize, usize, bool)>().prop_map(|(major, minor, patch, alpha)| {
            if alpha {
                exver::Version::new(
                    [0, major, minor]
                        .into_iter()
                        .chain(Some(patch).filter(|n| *n != 0)),
                    [],
                )
            } else {
                exver::Version::new([major, minor, patch], [])
            }
        })
    }

    fn versions() -> impl Strategy<Value = Version> {
        prop_oneof![
            Just(Version::V0_3_5(Wrapper(v0_3_5::Version::default()))),
            Just(Version::V0_3_5_1(Wrapper(v0_3_5_1::Version::default()))),
            Just(Version::V0_3_5_2(Wrapper(v0_3_5_2::Version::default()))),
            Just(Version::V0_3_6_alpha_0(Wrapper(
                v0_3_6_alpha_0::Version::default()
            ))),
            Just(Version::V0_3_6_alpha_1(Wrapper(
                v0_3_6_alpha_1::Version::default()
            ))),
            Just(Version::V0_3_6_alpha_2(Wrapper(
                v0_3_6_alpha_2::Version::default()
            ))),
            em_version().prop_map(Version::Other),
        ]
    }

    proptest! {
        #[test]
        fn exversion_isomorphic_version(original in em_version()) {
            let version = Version::from_exver_version(original.clone().into());
            let back = version.as_exver();
            prop_assert_eq!(original, back, "All versions should round trip");
        }
        #[test]
        fn version_isomorphic_em_version(version in versions()) {
            let sem_ver = version.as_exver();
            let back = Version::from_exver_version(sem_ver.into());
            prop_assert_eq!(format!("{:?}",version), format!("{:?}", back), "All versions should round trip");
        }
    }
}
