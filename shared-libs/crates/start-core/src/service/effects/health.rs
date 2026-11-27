use std::collections::BTreeSet;

use crate::db::model::DatabaseModel;
use crate::service::effects::prelude::*;
use crate::status::health_check::NamedHealthCheckResult;
use crate::{HealthCheckId, PackageId};

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetHealth {
    id: HealthCheckId,
    #[serde(flatten)]
    result: NamedHealthCheckResult,
}

rpc_toolkit::reflect_ts!(SetHealth);
rpc_toolkit::ts_export!(SetHealth, namespaces = [""]);
pub async fn set_health(
    context: EffectContext,
    SetHealth { id, result }: SetHealth,
) -> Result<(), Error> {
    let context = context.deref()?;

    let package_id = &context.seed.id;
    context
        .seed
        .ctx
        .db
        .mutate(move |db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(package_id)
                .or_not_found(package_id)?
                .as_status_info_mut()
                .as_health_mut()
                .insert(&id, &result)
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[visit(ts(
    type = "z.union([z.object({ only: z.array(z.string()) }), z.object({ except: z.array(z.string()) })])"
))]
pub struct ClearHealth {
    #[arg(
        long,
        num_args = 0..,
        conflicts_with = "except",
        required_unless_present = "except",
        help = "help.arg.only-health-checks"
    )]
    pub only: Option<Vec<HealthCheckId>>,
    #[arg(
        long,
        num_args = 0..,
        conflicts_with = "only",
        required_unless_present = "only",
        help = "help.arg.except-health-checks"
    )]
    pub except: Option<Vec<HealthCheckId>>,
}

rpc_toolkit::reflect_ts!(ClearHealth);
rpc_toolkit::ts_export!(ClearHealth, namespaces = [""]);

fn apply_clear_health(
    db: &mut DatabaseModel,
    package_id: &PackageId,
    ClearHealth { only, except }: ClearHealth,
) -> Result<(), Error> {
    let (ids, keep_listed) = match (only, except) {
        (Some(only), None) => (only, false),
        (None, Some(except)) => (except, true),
        (Some(_), Some(_)) => {
            return Err(Error::new(
                eyre!("cannot specify both `only` and `except`"),
                ErrorKind::InvalidRequest,
            ));
        }
        (None, None) => {
            return Err(Error::new(
                eyre!("must specify either `only` or `except`"),
                ErrorKind::InvalidRequest,
            ));
        }
    };
    let ids: BTreeSet<_> = ids.into_iter().collect();
    db.as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(package_id)
        .or_not_found(package_id)?
        .as_status_info_mut()
        .as_health_mut()
        .mutate(|health| Ok(health.retain(|id, _| ids.contains(id) == keep_listed)))
}

pub async fn clear_health(context: EffectContext, params: ClearHealth) -> Result<(), Error> {
    let context = context.deref()?;
    if matches!(&params, ClearHealth { only: Some(only), except: None } if only.is_empty()) {
        return Ok(());
    }
    let package_id = &context.seed.id;
    context
        .seed
        .ctx
        .db
        .mutate(move |db| apply_clear_health(db, package_id, params))
        .await
        .result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    fn health_db() -> DatabaseModel {
        DatabaseModel::from(json!({
            "public": {
                "packageData": {
                    "pkg": {
                        "statusInfo": {
                            "health": {
                                "worker-a": { "name": "Worker A", "result": "success", "message": null },
                                "worker-b": { "name": "Worker B", "result": "success", "message": null },
                                "worker-c": { "name": "Worker C", "result": "loading", "message": "syncing", "waiting": false },
                            },
                        },
                    },
                },
            },
        }))
    }

    fn health_ids(db: &DatabaseModel) -> Vec<String> {
        let pkg: PackageId = "pkg".parse().unwrap();
        db.as_public()
            .as_package_data()
            .as_idx(&pkg)
            .unwrap()
            .as_status_info()
            .as_health()
            .de()
            .unwrap()
            .keys()
            .map(|id| id.to_string())
            .collect()
    }

    #[test]
    fn clear_health_only_removes_targeted_ids() {
        let pkg: PackageId = "pkg".parse().unwrap();
        let mut db = health_db();

        apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: Some(vec!["worker-b".parse().unwrap()]),
                except: None,
            },
        )
        .unwrap();
        assert_eq!(health_ids(&db), vec!["worker-a", "worker-c"]);

        apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: Some(vec!["absent".parse().unwrap()]),
                except: None,
            },
        )
        .unwrap();
        assert_eq!(health_ids(&db), vec!["worker-a", "worker-c"]);

        apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: Some(vec![]),
                except: None,
            },
        )
        .unwrap();
        assert_eq!(health_ids(&db), vec!["worker-a", "worker-c"]);
    }

    #[test]
    fn clear_health_except_retains_only_listed_ids() {
        let pkg: PackageId = "pkg".parse().unwrap();
        let mut db = health_db();

        apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: None,
                except: Some(vec!["worker-a".parse().unwrap(), "absent".parse().unwrap()]),
            },
        )
        .unwrap();
        assert_eq!(health_ids(&db), vec!["worker-a"]);

        apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: None,
                except: Some(vec![]),
            },
        )
        .unwrap();
        assert!(health_ids(&db).is_empty());
    }

    #[test]
    fn clear_health_rejects_missing_or_ambiguous_filters() {
        let pkg: PackageId = "pkg".parse().unwrap();
        let mut db = health_db();

        let err = apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: None,
                except: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);

        let err = apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: Some(vec!["worker-a".parse().unwrap()]),
                except: Some(vec!["worker-b".parse().unwrap()]),
            },
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);

        let err = apply_clear_health(
            &mut db,
            &pkg,
            ClearHealth {
                only: Some(vec![]),
                except: Some(vec!["worker-a".parse().unwrap()]),
            },
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);

        assert_eq!(health_ids(&db), vec!["worker-a", "worker-b", "worker-c"]);
    }

    #[test]
    fn clear_health_serde_accepts_only_or_except_and_extra_metadata() {
        let only: ClearHealth =
            serde_json::from_value(serde_json::json!({ "only": ["worker-a"] })).unwrap();
        assert_eq!(
            only,
            ClearHealth {
                only: Some(vec!["worker-a".parse().unwrap()]),
                except: None,
            }
        );

        let only_with_event: ClearHealth =
            serde_json::from_value(serde_json::json!({ "only": ["worker-a"], "eventId": "evt-1" }))
                .unwrap();
        assert_eq!(only_with_event, only);

        let except: ClearHealth =
            serde_json::from_value(serde_json::json!({ "except": ["worker-b"] })).unwrap();
        assert_eq!(
            except,
            ClearHealth {
                only: None,
                except: Some(vec!["worker-b".parse().unwrap()]),
            }
        );

        let except_with_event: ClearHealth = serde_json::from_value(
            serde_json::json!({ "except": ["worker-b"], "eventId": "evt-1" }),
        )
        .unwrap();
        assert_eq!(except_with_event, except);
    }

    #[test]
    fn clear_health_cli_parses_only_and_except_and_round_trips_wire() {
        let pkg: PackageId = "pkg".parse().unwrap();

        let parsed_only =
            ClearHealth::try_parse_from(["clear-health", "--only", "worker-a", "worker-b"])
                .unwrap();
        assert_eq!(
            parsed_only,
            ClearHealth {
                only: Some(vec![
                    "worker-a".parse().unwrap(),
                    "worker-b".parse().unwrap(),
                ]),
                except: None,
            }
        );
        let repeated_only = ClearHealth::try_parse_from([
            "clear-health",
            "--only",
            "worker-a",
            "--only",
            "worker-b",
        ])
        .unwrap();
        assert_eq!(repeated_only, parsed_only);

        let wire_only: ClearHealth =
            serde_json::from_value(serde_json::to_value(&parsed_only).unwrap()).unwrap();
        let mut db = health_db();
        apply_clear_health(&mut db, &pkg, wire_only).unwrap();
        assert_eq!(health_ids(&db), vec!["worker-c"]);

        let parsed_except_all = ClearHealth::try_parse_from(["clear-health", "--except"]).unwrap();
        assert_eq!(
            parsed_except_all,
            ClearHealth {
                only: None,
                except: Some(vec![]),
            }
        );
        let wire_except_all: ClearHealth =
            serde_json::from_value(serde_json::to_value(&parsed_except_all).unwrap()).unwrap();
        apply_clear_health(&mut db, &pkg, wire_except_all).unwrap();
        assert!(health_ids(&db).is_empty());

        let parsed_only_empty = ClearHealth::try_parse_from(["clear-health", "--only"]).unwrap();
        assert_eq!(
            parsed_only_empty,
            ClearHealth {
                only: Some(vec![]),
                except: None,
            }
        );

        assert!(ClearHealth::try_parse_from(["clear-health"]).is_err());
        assert!(
            ClearHealth::try_parse_from([
                "clear-health",
                "--only",
                "worker-a",
                "--except",
                "worker-b"
            ])
            .is_err()
        );
        assert!(ClearHealth::try_parse_from(["clear-health", "--only", "INVALID ID"]).is_err());
    }
}
