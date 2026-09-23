use std::collections::BTreeSet;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use rust_i18n::t;

use crate::action::{ActionInput, ActionResult, display_action_result};
use crate::db::model::package::{
    ActionAccess, ActionMetadata, Task, TaskCondition, TaskEntry, TaskSeverity, TaskTrigger,
};
use crate::rpc_continuations::Guid;
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExportActionParams {
    id: ActionId,
    metadata: ActionMetadata,
}
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ClearActionsParams {
    #[arg(long, help = "help.arg.except-actions")]
    pub except: Vec<ActionId>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetActionInputParams {
    #[serde(default)]
    #[ts(skip)]
    #[arg(skip)]
    procedure_id: Guid,
    #[ts(optional)]
    #[arg(short, long, help = "help.arg.package-id")]
    package_id: Option<PackageId>,
    #[arg(help = "help.arg.action-id")]
    action_id: ActionId,
    #[ts(type = "Record<string, unknown> | null")]
    #[serde(default)]
    #[arg(skip)]
    prefill: Option<Value>,
}
async fn get_action_input(
    context: EffectContext,
    GetActionInputParams {
        procedure_id,
        package_id,
        action_id,
        prefill,
    }: GetActionInputParams,
) -> Result<Option<ActionInput>, Error> {
    let context = context.deref()?;
    let prefill = prefill.unwrap_or(Value::Null);
    let caller = Some(context.seed.id.clone());

    if let Some(package_id) = package_id {
        context
            .seed
            .ctx
            .services
            .get(&package_id)
            .await
            .as_ref()
            .or_not_found(&package_id)?
            .get_action_input(procedure_id, action_id, prefill, caller)
            .await
    } else {
        context
            .get_action_input(procedure_id, action_id, prefill, caller)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "EffectsRunActionParams")]
pub struct RunActionParams {
    #[serde(default)]
    #[ts(skip)]
    #[arg(skip)]
    procedure_id: Guid,
    #[ts(optional)]
    #[arg(short, long, help = "help.arg.package-id")]
    package_id: Option<PackageId>,
    /// The `eventId` from the `get-input` whose form this input answers.
    #[ts(optional)]
    #[arg(long, help = "help.arg.event-id")]
    event_id: Option<Guid>,
    #[arg(help = "help.arg.action-id")]
    action_id: ActionId,
    #[ts(type = "any")]
    #[arg(help = "help.arg.action-input")]
    input: Value,
}
async fn run_action(
    context: EffectContext,
    RunActionParams {
        procedure_id,
        package_id,
        event_id,
        action_id,
        input,
    }: RunActionParams,
) -> Result<Option<ActionResult>, Error> {
    let context = context.deref()?;
    let procedure_id = event_id.unwrap_or(procedure_id);
    let caller = Some(context.seed.id.clone());

    let package_id = package_id.as_ref().unwrap_or(&context.seed.id);

    if package_id != &context.seed.id {
        let db = context.seed.ctx.db.peek().await;
        let access = db
            .as_public()
            .as_package_data()
            .as_idx(package_id)
            .or_not_found(package_id)?
            .as_actions()
            .as_idx(&action_id)
            .or_not_found(&action_id)?
            .as_access()
            .de()?
            .unwrap_or_default();
        match access {
            ActionAccess::Public => {}
            ActionAccess::Dependent => {
                let caller_depends_on_target = db
                    .as_public()
                    .as_package_data()
                    .as_idx(&context.seed.id)
                    .or_not_found(&context.seed.id)?
                    .as_current_dependencies()
                    .de()?
                    .0
                    .contains_key(package_id);
                if !caller_depends_on_target {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "service.effects.action.action-only-accessible-to-dependents",
                                action_id = action_id,
                                package_id = package_id
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
            }
            ActionAccess::User => {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "service.effects.action.action-not-accessible",
                            action_id = action_id,
                            package_id = package_id
                        )
                    ),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
        context
            .seed
            .ctx
            .services
            .get(package_id)
            .await
            .as_ref()
            .or_not_found(package_id)?
            .run_action(procedure_id, action_id, input, caller)
            .await
    } else {
        context
            .run_action(procedure_id, action_id, input, caller)
            .await
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateTaskParams {
    #[serde(default)]
    #[ts(skip)]
    procedure_id: Guid,
    replay_id: ReplayId,
    #[serde(flatten)]
    task: Task,
}
async fn create_task(
    context: EffectContext,
    CreateTaskParams {
        procedure_id,
        replay_id,
        task,
    }: CreateTaskParams,
) -> Result<(), Error> {
    let context = context.deref()?;

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
                // try_get to avoid deadlock during self-calls on install
                let guard = context.seed.ctx.services.try_get(&task.package_id);
                let prev = if let Some(service) = guard
                    .as_deref()
                    .and_then(Option::as_ref)
                    .filter(|s| s.is_initialized())
                {
                    service
                        .get_action_input(
                            procedure_id.clone(),
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
                    // Action or service not available (e.g. still initializing during
                    // server boot), so the condition can't be evaluated yet. Resolved
                    // below from the prior entry for this replay_id; retested when the
                    // action is exported (export_action) or the service's init
                    // completes (Service::recheck_tasks).
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
                // Unknown: a replayed task keeps its previous state rather than
                // force-stopping the service on an assumption (the common case is
                // server boot, where the target service initializes later and the
                // recheck then corrects the state). A brand new task is
                // conservatively active.
                None => match pde.as_tasks().as_idx(&replay_id) {
                    Some(entry) => entry.as_active().de()?,
                    None => true,
                },
            };
            if active && task.severity == TaskSeverity::Critical {
                pde.as_status_info_mut().stop()?;
            }
            pde.as_tasks_mut()
                .insert(&replay_id, &TaskEntry { active, task })
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[ts(type = "{ only: string[] } | { except: string[] }")]
#[ts(export)]
pub struct ClearTasksParams {
    #[arg(long, conflicts_with = "except", help = "help.arg.only-tasks")]
    pub only: Option<Vec<ReplayId>>,
    #[arg(long, conflicts_with = "only", help = "help.arg.except-tasks")]
    pub except: Option<Vec<ReplayId>>,
}

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
