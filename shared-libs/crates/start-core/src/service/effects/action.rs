use std::collections::BTreeSet;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use rust_i18n::t;

use crate::action::{ActionInput, ActionResult, display_action_result};
use crate::db::model::DatabaseModel;
use crate::db::model::package::{
    ActionAccess, ActionMetadata, Task, TaskCondition, TaskEntry, TaskTrigger,
};
use crate::dependencies::providers::{find_package, resolve_package};
use crate::service::cli::ContainerCliContext;
use crate::service::effects::prelude::*;
use crate::util::serde::HandlerExtSerde;
use crate::{ActionId, PackageId, ReplayId};

pub fn action_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("export", from_fn_async(export_action).no_cli())
        .subcommand(
            "clear",
            from_fn_async(clear_actions)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-input",
            from_fn_async(get_action_input)
                .with_display_serializable()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "run",
            from_fn_async(run_action)
                .with_display_serializable()
                .with_custom_display_fn(|args, res| display_action_result(args.params, res))
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand("create-task", from_fn_async(create_task).no_cli())
        .subcommand(
            "clear-tasks",
            from_fn_async(clear_tasks)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ExportActionParams {
    id: ActionId,
    metadata: ActionMetadata,
}

rpc_toolkit::reflect_ts!(ExportActionParams);
rpc_toolkit::ts_export!(ExportActionParams, namespaces = [""]);
pub async fn export_action(
    context: EffectContext,
    ExportActionParams { id, metadata }: ExportActionParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let package_id = context.seed.id.clone();
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let model = db
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&package_id)
                .or_not_found(&package_id)?
                .as_actions_mut();
            let mut value = model.de()?;
            value.insert(id, metadata);
            model.ser(&value)
        })
        .await
        .result?;
    context.recheck_tasks().await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct ClearActionsParams {
    #[arg(long, help = "help.arg.except-actions")]
    pub except: Vec<ActionId>,
}

rpc_toolkit::reflect_ts!(ClearActionsParams);
rpc_toolkit::ts_export!(ClearActionsParams, namespaces = [""]);

async fn clear_actions(
    context: EffectContext,
    ClearActionsParams { except }: ClearActionsParams,
) -> Result<(), Error> {
    let except: BTreeSet<_> = except.into_iter().collect();
    let context = context.deref()?;
    let package_id = context.seed.id.clone();
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&package_id)
                .or_not_found(&package_id)?
                .as_actions_mut()
                .mutate(|a| Ok(a.retain(|e, _| except.contains(e))))
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct GetActionInputParams {
    #[arg(short, long, help = "help.arg.package-id")]
    package_id: Option<PackageId>,
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    #[arg(skip)]
    event: EventId,
    #[arg(help = "help.arg.action-id")]
    action_id: ActionId,
    #[visit(ts(type = "z.record(z.string(), z.unknown()).nullable()"))]
    #[serde(default)]
    #[arg(skip)]
    prefill: Option<Value>,
}

rpc_toolkit::reflect_ts!(GetActionInputParams);
rpc_toolkit::ts_export!(GetActionInputParams, namespaces = [""]);
async fn get_action_input(
    context: EffectContext,
    GetActionInputParams {
        event,
        package_id,
        action_id,
        prefill,
    }: GetActionInputParams,
) -> Result<Option<ActionInput>, Error> {
    let context = context.deref()?;
    let event_id = event.or_new();
    let prefill = prefill.unwrap_or(Value::Null);
    let caller = Some(context.seed.id.clone());

    let package_id = check_action_access(
        &context.seed.ctx.db.peek().await,
        &context.seed.id,
        package_id.as_ref().unwrap_or(&context.seed.id),
        &action_id,
    )?;

    if package_id != context.seed.id {
        context
            .seed
            .ctx
            .services
            .get(&package_id)
            .await
            .as_ref()
            .or_not_found(&package_id)?
            .get_action_input(event_id, action_id, prefill, caller)
            .await
    } else {
        context
            .get_action_input(event_id, action_id, prefill, caller)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[visit(ts(rename = "EffectsRunActionParams"))]
pub struct RunActionParams {
    #[arg(short, long, help = "help.arg.package-id")]
    package_id: Option<PackageId>,
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    #[command(flatten)]
    event: EventId,
    #[arg(help = "help.arg.action-id")]
    action_id: ActionId,
    #[arg(help = "help.arg.action-input")]
    input: Value,
}

rpc_toolkit::reflect_ts!(RunActionParams);
rpc_toolkit::ts_export!(RunActionParams, namespaces = [""]);
async fn run_action(
    context: EffectContext,
    RunActionParams {
        event,
        package_id,
        action_id,
        input,
    }: RunActionParams,
) -> Result<Option<ActionResult>, Error> {
    let context = context.deref()?;
    let event_id = event.or_new();
    let caller = Some(context.seed.id.clone());

    let package_id = check_action_access(
        &context.seed.ctx.db.peek().await,
        &context.seed.id,
        package_id.as_ref().unwrap_or(&context.seed.id),
        &action_id,
    )?;

    if package_id != context.seed.id {
        context
            .seed
            .ctx
            .services
            .get(&package_id)
            .await
            .as_ref()
            .or_not_found(&package_id)?
            .run_action(event_id, action_id, input, caller)
            .await
    } else {
        context.run_action(event_id, action_id, input, caller).await
    }
}

fn check_action_access(
    db: &DatabaseModel,
    caller_id: &PackageId,
    package_id: &PackageId,
    action_id: &ActionId,
) -> Result<PackageId, Error> {
    let resolved = resolve_package(db, caller_id, package_id)?;
    let package_id = &resolved;
    if package_id == caller_id {
        return Ok(resolved);
    }

    let access = db
        .as_public()
        .as_package_data()
        .as_idx(package_id)
        .or_not_found(package_id)?
        .as_actions()
        .as_idx(action_id)
        .or_not_found(action_id)?
        .as_access()
        .de()?
        .unwrap_or_default();
    match access {
        ActionAccess::Public => Ok(resolved),
        ActionAccess::Dependent => {
            let dependencies = db
                .as_public()
                .as_package_data()
                .as_idx(caller_id)
                .or_not_found(caller_id)?
                .as_current_dependencies()
                .de()?;
            let mut caller_depends_on_target = false;
            for logical_id in dependencies.0.keys() {
                if find_package(db, caller_id, logical_id)?.as_ref() == Some(package_id) {
                    caller_depends_on_target = true;
                    break;
                }
            }
            if caller_depends_on_target {
                Ok(resolved)
            } else {
                Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "service.effects.action.action-only-accessible-to-dependents",
                            action_id = action_id,
                            package_id = package_id
                        )
                    ),
                    ErrorKind::InvalidRequest,
                ))
            }
        }
        ActionAccess::User => Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "service.effects.action.action-not-accessible",
                    action_id = action_id,
                    package_id = package_id
                )
            ),
            ErrorKind::InvalidRequest,
        )),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskParams {
    #[serde(flatten)]
    #[visit(opaque, ts(skip))]
    event: EventId,
    replay_id: ReplayId,
    #[serde(flatten)]
    task: Task,
}

rpc_toolkit::reflect_ts!(CreateTaskParams);
rpc_toolkit::ts_export!(CreateTaskParams, namespaces = [""]);
async fn create_task(
    context: EffectContext,
    CreateTaskParams {
        event,
        replay_id,
        task,
    }: CreateTaskParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let event_id = event.or_new();

    let src_id = &context.seed.id;
    let active = match &task.when {
        Some(TaskTrigger { once, condition }) => match condition {
            TaskCondition::InputNotMatches => {
                let Some(input) = task.input.as_ref() else {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!("service.effects.action.input-not-matches-requires-input")
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                };
                let target =
                    find_package(&context.seed.ctx.db.peek().await, src_id, &task.package_id)?;
                // try_get avoids deadlock during self-calls on install.
                let guard = target
                    .as_ref()
                    .and_then(|id| context.seed.ctx.services.try_get(id));
                let prev = if let Some(service) = guard
                    .as_deref()
                    .and_then(Option::as_ref)
                    .filter(|s| s.is_initialized())
                {
                    service
                        .get_action_input(
                            event_id.clone(),
                            task.action_id.clone(),
                            Value::Null,
                            None,
                        )
                        .await
                        .log_err()
                        .flatten()
                } else {
                    None
                };
                if let Some(prev) = prev {
                    if input.matches(prev.value.as_ref()) {
                        if *once {
                            return Ok(());
                        } else {
                            Some(false)
                        }
                    } else {
                        Some(true)
                    }
                } else {
                    // Action export and service initialization recheck unresolved conditions.
                    None
                }
            }
        },
        None => Some(true),
    };
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            let pde = db
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(src_id)
                .or_not_found(src_id)?;
            let active = match active {
                Some(active) => active,
                None => match pde.as_tasks().as_idx(&replay_id) {
                    Some(entry) => entry.as_active().de()?,
                    None => true,
                },
            };
            pde.as_tasks_mut()
                .insert(&replay_id, &TaskEntry { active, task })
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[visit(ts(
    type = "z.union([z.object({ only: z.array(z.string()) }), z.object({ except: z.array(z.string()) })])"
))]
pub struct ClearTasksParams {
    #[arg(long, conflicts_with = "except", help = "help.arg.only-tasks")]
    pub only: Option<Vec<ReplayId>>,
    #[arg(long, conflicts_with = "only", help = "help.arg.except-tasks")]
    pub except: Option<Vec<ReplayId>>,
}

rpc_toolkit::reflect_ts!(ClearTasksParams);
rpc_toolkit::ts_export!(ClearTasksParams, namespaces = [""]);

async fn clear_tasks(
    context: EffectContext,
    ClearTasksParams { only, except }: ClearTasksParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let package_id = context.seed.id.clone();
    let only = only.map(|only| only.into_iter().collect::<BTreeSet<_>>());
    let except = except.map(|except| except.into_iter().collect::<BTreeSet<_>>());
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&package_id)
                .or_not_found(&package_id)?
                .as_tasks_mut()
                .mutate(|a| {
                    Ok(a.retain(|e, _| {
                        only.as_ref().map_or(true, |only| !only.contains(e))
                            && except.as_ref().map_or(true, |except| except.contains(e))
                    }))
                })
        })
        .await
        .result?;
    Ok(())
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;
    use crate::rpc_continuations::Guid;

    fn action_access_db(access: Option<ActionAccess>, dependencies: Value) -> DatabaseModel {
        DatabaseModel::from(json!({
            "public": {
                "packageData": {
                    "caller": {
                        "currentDependencies": dependencies,
                        "dependencyBindings": {
                            "target": { "providerId": "target", "versions": ["1.0.0:0"], "candidates": {} }
                        }
                    },
                    "target": { "actions": { "configure": { "access": access } } },
                },
            },
        }))
    }

    #[test]
    fn action_access_checks_current_dependencies_for_cross_package_calls() {
        let caller: PackageId = "caller".parse().unwrap();
        let target: PackageId = "target".parse().unwrap();
        let action: ActionId = "configure".parse().unwrap();
        for (dependencies, is_dependent) in [
            (json!({}), false),
            (
                json!({ "other": { "kind": "exists", "versionRange": "*" } }),
                false,
            ),
            (
                json!({ "target": { "kind": "exists", "versionRange": "*" } }),
                true,
            ),
            (
                json!({ "target": { "kind": "running", "versionRange": "*" } }),
                true,
            ),
        ] {
            for (access, allowed) in [
                (Some(ActionAccess::Public), true),
                (Some(ActionAccess::Dependent), is_dependent),
                (Some(ActionAccess::User), false),
                (None, false),
            ] {
                let db = action_access_db(access, dependencies.clone());
                let result = check_action_access(&db, &caller, &target, &action);
                if allowed {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidRequest);
                }
            }
        }
    }

    #[test]
    fn action_access_uses_selected_providers_policy_and_dependent_membership() {
        let caller: PackageId = "caller".parse().unwrap();
        let logical: PackageId = "logical".parse().unwrap();
        let selected: PackageId = "selected".parse().unwrap();
        let other: PackageId = "other".parse().unwrap();
        let action: ActionId = "configure".parse().unwrap();
        for access in [
            ActionAccess::Public,
            ActionAccess::Dependent,
            ActionAccess::User,
        ] {
            let allowed = !matches!(access, ActionAccess::User);
            let db = DatabaseModel::from(json!({
                "public": { "packageData": {
                    "caller": {
                        "currentDependencies": { "logical": { "kind": "exists", "versionRange": "*" } },
                        "dependencyBindings": { "logical": {
                            "providerId": "selected", "versions": ["1.0.0:0"],
                            "candidates": { "selected": ["1.0.0"], "other": ["1.0.0"] }
                        } }
                    },
                    "selected": { "actions": { "configure": { "access": access } } },
                    "other": { "actions": { "configure": { "access": "dependent" } } }
                } }
            }));
            let result = check_action_access(&db, &caller, &logical, &action);
            if allowed {
                assert_eq!(result.unwrap(), selected);
            } else {
                assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidRequest);
            }
            assert_eq!(
                check_action_access(&db, &caller, &other, &action)
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidRequest
            );
        }
    }

    #[test]
    fn action_access_does_not_fall_back_to_an_unselected_logical_package() {
        let db = DatabaseModel::from(json!({
            "public": { "packageData": {
                "caller": {
                    "currentDependencies": { "logical": { "kind": "exists", "versionRange": "*" } },
                    "dependencyBindings": { "logical": { "providerId": null, "versions": [], "candidates": {} } }
                },
                "logical": { "actions": { "configure": { "access": "public" } } }
            } }
        }));
        assert_eq!(
            check_action_access(
                &db,
                &"caller".parse().unwrap(),
                &"logical".parse().unwrap(),
                &"configure".parse().unwrap(),
            )
            .unwrap_err()
            .kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn action_access_allows_self_calls_for_every_access_level() {
        let target: PackageId = "target".parse().unwrap();
        let action: ActionId = "configure".parse().unwrap();
        for access in [
            Some(ActionAccess::Public),
            Some(ActionAccess::Dependent),
            Some(ActionAccess::User),
            None,
        ] {
            let db = action_access_db(access, json!({}));
            check_action_access(&db, &target, &target, &action).unwrap();
        }
    }

    #[test]
    fn action_access_defaults_omitted_access_to_user() {
        let db = DatabaseModel::from(json!({
            "public": { "packageData": {
                "caller": { "currentDependencies": {} },
                "target": { "actions": { "configure": {} } }
            } },
        }));
        let error = check_action_access(
            &db,
            &"caller".parse().unwrap(),
            &"target".parse().unwrap(),
            &"configure".parse().unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn action_access_rejects_missing_packages_and_actions() {
        let caller: PackageId = "caller".parse().unwrap();
        let action: ActionId = "configure".parse().unwrap();
        let db = action_access_db(Some(ActionAccess::Public), json!({}));
        for (target, action) in [
            ("missing".parse().unwrap(), action),
            ("target".parse().unwrap(), "missing".parse().unwrap()),
        ] {
            let error = check_action_access(&db, &caller, &target, &action).unwrap_err();
            assert_eq!(error.kind, ErrorKind::NotFound);
        }
    }

    fn envelope(params: Value) -> (Guid, Value) {
        let event_id = Guid::new();
        let mut params = params;
        params["eventId"] = json!(event_id.as_ref());
        (event_id, params)
    }

    #[test]
    fn effect_params_take_the_calling_procedures_event_id() {
        let (id, params) = envelope(json!({ "actionId": "attach", "prefill": null }));
        let get: GetActionInputParams = imbl_value::from_value(params).unwrap();
        assert_eq!(get.event.or_new(), id);

        let (id, params) = envelope(json!({ "actionId": "attach", "input": { "a": 1 } }));
        let run: RunActionParams = imbl_value::from_value(params).unwrap();
        assert_eq!(run.event.or_new(), id);
        assert_eq!(run.input, json!({ "a": 1 }));

        let (id, params) = envelope(json!({
            "replayId": "r",
            "packageId": "tor",
            "actionId": "attach",
        }));
        let task: CreateTaskParams = imbl_value::from_value(params).unwrap();
        assert_eq!(task.event.or_new(), id);
        assert_eq!(AsRef::<str>::as_ref(&task.task.action_id), "attach");
    }

    #[test]
    fn cli_event_id_reaches_the_request() {
        let id = Guid::new();

        let run =
            RunActionParams::try_parse_from(["run", "--event-id", id.as_ref(), "attach", "{}"])
                .unwrap();
        let sent = imbl_value::to_value(&run).unwrap();
        assert_eq!(sent["eventId"], json!(id.as_ref()));
        assert_eq!(
            imbl_value::from_value::<RunActionParams>(sent)
                .unwrap()
                .event
                .or_new(),
            id
        );

        assert!(
            GetActionInputParams::try_parse_from([
                "get-input",
                "--event-id",
                id.as_ref(),
                "attach",
            ])
            .is_err()
        );

        let unnamed = RunActionParams::try_parse_from(["run", "attach", "{}"]).unwrap();
        assert!(unnamed.event.event_id.is_none());
        assert!(imbl_value::to_value(&unnamed).unwrap()["eventId"].is_null());
    }
}
