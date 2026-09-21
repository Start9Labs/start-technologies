use std::collections::BTreeMap;
use std::time::Duration;

use imbl_value::json;

use crate::action::{ActionInput, ActionResult};
use crate::db::model::package::{
    ActionVisibility, AllowedStatuses, TaskCondition, TaskEntry, TaskInput, TaskSeverity,
};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::service::{ProcedureName, Service, ServiceActor};
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::actor::{ConflictBuilder, Handler};
use crate::util::serde::is_partial_of;
use crate::{ActionId, PackageId, ReplayId};

pub(super) struct GetActionInput {
    id: ActionId,
    prefill: Value,
    caller: Option<PackageId>,
}
impl Handler<GetActionInput> for ServiceActor {
    type Response = Result<Option<ActionInput>, Error>;
    fn conflicts_with(_: &GetActionInput) -> ConflictBuilder<Self> {
        ConflictBuilder::nothing()
    }
    async fn handle(
        &mut self,
        id: Guid,
        GetActionInput {
            id: action_id,
            prefill,
            caller,
        }: GetActionInput,
        _: &BackgroundJobQueue,
    ) -> Self::Response {
        let container = &self.0.persistent_container;
        container
            .execute::<Option<ActionInput>>(
                id,
                ProcedureName::GetActionInput(action_id),
                json!({ "prefill": prefill, "caller": caller }),
                Some(Duration::from_secs(30)),
            )
            .await
            .with_kind(ErrorKind::Action)
    }
}

impl Service {
    /// `caller` is the service asking through its effects, or `None` for the
    /// user and for the OS evaluating a task.
    pub async fn get_action_input(
        &self,
        id: Guid,
        action_id: ActionId,
        prefill: Value,
        caller: Option<PackageId>,
    ) -> Result<Option<ActionInput>, Error> {
        if !self
            .seed
            .ctx
            .db
            .peek()
            .await
            .as_public()
            .as_package_data()
            .as_idx(&self.seed.id)
            .or_not_found(&self.seed.id)?
            .as_actions()
            .as_idx(&action_id)
            .or_not_found(&action_id)?
            .as_has_input()
            .de()?
        {
            return Ok(None);
        }
        self.actor
            .send(
                id,
                GetActionInput {
                    id: action_id,
                    prefill,
                    caller,
                },
            )
            .await?
    }
}

fn conflicts(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => left.iter().any(|(k, v)| {
            if let Some(v_right) = right.get(k) {
                conflicts(v, v_right)
            } else {
                false
            }
        }),
        (Value::Array(left), Value::Array(right)) => left
            .iter()
            .any(|v| right.iter().all(|v_right| conflicts(v, v_right))),
        (_, _) => left != right,
    }
}

pub fn update_tasks(
    tasks: &mut BTreeMap<ReplayId, TaskEntry>,
    package_id: &PackageId,
    action_id: &ActionId,
    input: &Value,
    was_run: bool,
) -> bool {
    let mut critical_activated = false;
    tasks.retain(|_, v| {
        if &v.task.package_id != package_id || &v.task.action_id != action_id {
            return true;
        }
        if let Some(when) = &v.task.when {
            match &when.condition {
                TaskCondition::InputNotMatches => match &v.task.input {
                    Some(TaskInput::Partial { accept, .. }) => {
                        if accept.iter().any(|a| is_partial_of(a, input)) {
                            if when.once {
                                return !was_run;
                            } else {
                                v.active = false;
                            }
                        } else if accept.iter().all(|a| conflicts(a, input)) {
                            v.active = true;
                            if v.task.severity == TaskSeverity::Critical {
                                critical_activated = true;
                            }
                        }
                    }
                    None => {
                        tracing::error!(
                            "{}",
                            t!(
                                "service.action.action-request-invalid-state",
                                task = format!("{:?}", v.task)
                            )
                        );
                    }
                },
            }
            true
        } else {
            !was_run
        }
    });
    critical_activated
}

pub(super) struct RunAction {
    action_id: ActionId,
    input: Value,
    caller: Option<PackageId>,
}
impl Handler<RunAction> for ServiceActor {
    type Response = Result<Option<ActionResult>, Error>;
    fn conflicts_with(_: &RunAction) -> ConflictBuilder<Self> {
        ConflictBuilder::everything().except::<GetActionInput>()
    }
    async fn handle(
        &mut self,
        id: Guid,
        RunAction {
            ref action_id,
            input,
            caller,
        }: RunAction,
        _: &BackgroundJobQueue,
    ) -> Self::Response {
        let container = &self.0.persistent_container;
        let package_id = &self.0.id;
        let pde = self
            .0
            .ctx
            .db
            .peek()
            .await
            .into_public()
            .into_package_data()
            .into_idx(package_id)
            .or_not_found(package_id)?;
        let action = pde
            .as_actions()
            .as_idx(action_id)
            .or_not_found(lazy_format!("{package_id} action {action_id}"))?
            .de()?;
        if matches!(&action.visibility, ActionVisibility::Disabled(_)) {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("service.action.action-is-disabled", action_id = action_id)
                ),
                ErrorKind::Action,
            ));
        }
        let running = pde.as_status_info().as_started().transpose_ref().is_some();
        if match action.allowed_statuses {
            AllowedStatuses::OnlyRunning => !running,
            AllowedStatuses::OnlyStopped => running,
            _ => false,
        } {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "service.action.service-not-in-allowed-status",
                        action_id = action_id
                    )
                ),
                ErrorKind::Action,
            ));
        }
        let result = container
            .execute::<Option<ActionResult>>(
                id.clone(),
                ProcedureName::RunAction(action_id.clone()),
                json!({
                    "input": input,
                    "caller": caller,
                }),
                Some(Duration::from_secs(120)),
            )
            .await
            .with_kind(ErrorKind::Action)?;
        let package_id = package_id.clone();
        self.0
            .ctx
            .db
            .mutate(|db| {
                for (_, pde) in db.as_public_mut().as_package_data_mut().as_entries_mut()? {
                    if pde.as_tasks_mut().mutate(|tasks| {
                        Ok(update_tasks(tasks, &package_id, action_id, &input, true))
                    })? {
                        pde.as_status_info_mut().stop()?;
                    }
                }
                Ok(())
            })
            .await
            .result?;
        Ok(result)
    }
}

impl Service {
    /// `caller` is the service running the action through its effects, or
    /// `None` for the user.
    pub async fn run_action(
        &self,
        id: Guid,
        action_id: ActionId,
        input: Value,
        caller: Option<PackageId>,
    ) -> Result<Option<ActionResult>, Error> {
        self.actor
            .send(
                id,
                RunAction {
                    action_id,
                    input,
                    caller,
                },
            )
            .await?
    }
}
