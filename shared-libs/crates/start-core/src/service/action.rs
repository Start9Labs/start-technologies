use std::collections::BTreeMap;
use std::time::Duration;

use imbl_value::json;

use crate::action::{ActionInput, ActionResult};
use crate::db::model::DatabaseModel;
use crate::db::model::package::{
    ActionVisibility, AllowedStatuses, TaskCondition, TaskEntry, TaskInput,
};
use crate::dependencies::providers::find_package;
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
        event_id: Guid,
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
                event_id,
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
        event_id: Guid,
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
                event_id,
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
) {
    update_tasks_matching(tasks, |id| id == package_id, action_id, input, was_run);
}

fn update_tasks_matching(
    tasks: &mut BTreeMap<ReplayId, TaskEntry>,
    mut matches_target: impl FnMut(&PackageId) -> bool,
    action_id: &ActionId,
    input: &Value,
    was_run: bool,
) {
    tasks.retain(|_, v| {
        if !matches_target(&v.task.package_id) || &v.task.action_id != action_id {
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
}

pub(super) fn task_targets_service(
    db: &DatabaseModel,
    owner: &PackageId,
    requested: &PackageId,
    service_id: &PackageId,
) -> Result<bool, Error> {
    Ok(find_package(db, owner, requested)?.as_ref() == Some(service_id))
}

/// Inactive dependency tasks have no effective target.
pub(crate) fn conditional_task_targets(
    db: &DatabaseModel,
    owner: &PackageId,
) -> Result<BTreeMap<ReplayId, Option<PackageId>>, Error> {
    let Some(package) = db.as_public().as_package_data().as_idx(owner) else {
        return Ok(BTreeMap::new());
    };
    let dependencies = package.as_current_dependencies().de()?;
    package
        .as_tasks()
        .de()?
        .into_iter()
        .filter(|(_, entry)| entry.task.when.is_some())
        .map(|(id, entry)| {
            let target = if dependencies.is_task_target(owner, &entry.task.package_id) {
                find_package(db, owner, &entry.task.package_id)?
            } else {
                None
            };
            Ok((id, target))
        })
        .collect()
}

/// A condition observed on one provider cannot suppress a task on another.
pub(crate) fn reset_task_conditions(
    db: &mut DatabaseModel,
    owner: &PackageId,
    previous: BTreeMap<ReplayId, Option<PackageId>>,
) -> Result<bool, Error> {
    let targets = conditional_task_targets(db, owner)?;
    if targets == previous {
        return Ok(false);
    }
    db.as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(owner)
        .or_not_found(owner)?
        .as_tasks_mut()
        .mutate(|tasks| {
            for (id, previous) in previous {
                if targets.get(&id) != Some(&previous) {
                    if let Some(task) = tasks.get_mut(&id) {
                        task.active = true;
                    }
                }
            }
            Ok(())
        })?;
    Ok(true)
}

/// Task package IDs retain logical intent across provider changes.
pub(crate) fn update_tasks_for_service(
    db: &mut DatabaseModel,
    service_id: &PackageId,
    action_id: &ActionId,
    input: &Value,
    was_run: bool,
) -> Result<(), Error> {
    for owner in db.as_public().as_package_data().keys()? {
        let mut tasks = db
            .as_public()
            .as_package_data()
            .as_idx(&owner)
            .or_not_found(&owner)?
            .as_tasks()
            .de()?;
        let mut targets = std::collections::BTreeSet::new();
        for entry in tasks.values() {
            if task_targets_service(db, &owner, &entry.task.package_id, service_id)? {
                targets.insert(entry.task.package_id.clone());
            }
        }
        update_tasks_matching(
            &mut tasks,
            |id| targets.contains(id),
            action_id,
            input,
            was_run,
        );
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&owner)
            .or_not_found(&owner)?
            .as_tasks_mut()
            .ser(&tasks)?;
    }
    Ok(())
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
        event_id: Guid,
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
                event_id.clone(),
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
            .mutate(|db| update_tasks_for_service(db, &package_id, action_id, &input, true))
            .await
            .result?;
        if let Some(result) = &result {
            self.0
                .action_files
                .publish(&self.0.ctx.rpc_continuations, &event_id, result)
                .await?;
        }
        Ok(result)
    }
}

impl Service {
    /// `caller` is the service running the action through its effects, or
    /// `None` for the user.
    pub async fn run_action(
        &self,
        event_id: Guid,
        action_id: ActionId,
        input: Value,
        caller: Option<PackageId>,
    ) -> Result<Option<ActionResult>, Error> {
        self.actor
            .send(
                event_id,
                RunAction {
                    action_id,
                    input,
                    caller,
                },
            )
            .await?
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn task_db() -> DatabaseModel {
        let owner = |provider: Option<&str>| {
            json!({
                "currentDependencies": { "logical": { "kind": "exists", "versionRange": "*" } },
                "dependencyBindings": { "logical": {
                    "providerId": provider, "versions": ["1.0.0:0"], "candidates": {}
                } },
                "tasks": {
                    "configure": {
                        "active": true,
                        "task": {
                            "packageId": "logical", "actionId": "configure",
                            "when": { "condition": "input-not-matches", "once": false },
                            "input": { "kind": "partial", "accept": [{ "enabled": true }], "set": {} }
                        }
                    },
                    "once": { "active": true, "task": { "packageId": "logical", "actionId": "configure" } },
                    "conditionalOnce": {
                        "active": true,
                        "task": {
                            "packageId": "logical", "actionId": "configure",
                            "when": { "condition": "input-not-matches", "once": true },
                            "input": { "kind": "partial", "accept": [{ "enabled": true }], "set": {} }
                        }
                    },
                    "unrelated": { "active": true, "task": { "packageId": "logical", "actionId": "other" } }
                }
            })
        };
        DatabaseModel::from(json!({
            "public": { "packageData": {
                "consumer": owner(Some("provider")),
                "different": owner(Some("alternate")),
                "unresolved": owner(None),
                "provider": { "currentDependencies": {}, "dependencyBindings": {}, "tasks": {
                    "self": { "active": true, "task": { "packageId": "provider", "actionId": "configure" } }
                } }
            } }
        }))
    }

    fn owner_tasks(db: &DatabaseModel, owner: &str) -> BTreeMap<ReplayId, TaskEntry> {
        db.as_public()
            .as_package_data()
            .as_idx(&owner.parse().unwrap())
            .unwrap()
            .as_tasks()
            .de()
            .unwrap()
    }

    #[test]
    fn optional_reactivation_rechecks_conditional_alias_tasks() {
        let mut db = task_db();
        let owner: PackageId = "consumer".parse().unwrap();
        let provider: PackageId = "provider".parse().unwrap();
        let action: ActionId = "configure".parse().unwrap();
        let requirements = db
            .as_public()
            .as_package_data()
            .as_idx(&owner)
            .unwrap()
            .as_current_dependencies()
            .de()
            .unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&owner)
            .unwrap()
            .as_tasks_mut()
            .mutate(|tasks| {
                tasks
                    .get_mut(&"configure".parse().unwrap())
                    .unwrap()
                    .task
                    .severity = crate::db::model::package::TaskSeverity::Critical;
                Ok(())
            })
            .unwrap();
        update_tasks_for_service(&mut db, &provider, &action, &json!({"enabled":true}), false)
            .unwrap();
        assert!(!owner_tasks(&db, "consumer")[&"configure".parse().unwrap()].active);
        let previous = conditional_task_targets(&db, &owner).unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&owner)
            .unwrap()
            .as_current_dependencies_mut()
            .ser(&crate::db::model::package::CurrentDependencies::default())
            .unwrap();
        reset_task_conditions(&mut db, &owner, previous).unwrap();
        assert!(
            !db.as_public()
                .as_package_data()
                .as_idx(&owner)
                .unwrap()
                .has_blocking_task(&owner)
                .unwrap()
        );
        let previous = conditional_task_targets(&db, &owner).unwrap();
        db.as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&owner)
            .unwrap()
            .as_current_dependencies_mut()
            .ser(&requirements)
            .unwrap();
        reset_task_conditions(&mut db, &owner, previous).unwrap();
        assert!(
            db.as_public()
                .as_package_data()
                .as_idx(&owner)
                .unwrap()
                .has_blocking_task(&owner)
                .unwrap()
        );
        update_tasks_for_service(&mut db, &provider, &action, &json!({"enabled":true}), false)
            .unwrap();
        assert!(
            !db.as_public()
                .as_package_data()
                .as_idx(&owner)
                .unwrap()
                .has_blocking_task(&owner)
                .unwrap()
        );
        assert_eq!(
            owner_tasks(&db, "consumer")[&"configure".parse().unwrap()]
                .task
                .package_id,
            "logical".parse().unwrap()
        );
    }

    #[test]
    fn removed_owner_has_no_conditional_task_targets() {
        assert!(
            conditional_task_targets(&task_db(), &"removed".parse().unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn task_rechecks_resolve_per_owner_and_preserve_logical_intent() {
        let mut db = task_db();
        let provider = "provider".parse().unwrap();
        let action = "configure".parse().unwrap();
        update_tasks_for_service(
            &mut db,
            &provider,
            &action,
            &json!({ "enabled": true }),
            false,
        )
        .unwrap();
        let tasks = owner_tasks(&db, "consumer");
        assert!(!tasks[&"configure".parse().unwrap()].active);
        assert_eq!(
            tasks[&"configure".parse().unwrap()].task.package_id,
            "logical".parse().unwrap()
        );
        assert!(tasks.contains_key(&"once".parse().unwrap()));
        assert!(tasks.contains_key(&"conditionalOnce".parse().unwrap()));
        for owner in ["different", "unresolved"] {
            assert!(owner_tasks(&db, owner).values().all(|entry| entry.active));
        }
        update_tasks_for_service(
            &mut db,
            &provider,
            &action,
            &json!({ "enabled": false }),
            false,
        )
        .unwrap();
        assert!(owner_tasks(&db, "consumer")[&"configure".parse().unwrap()].active);
    }

    #[test]
    fn action_completion_affects_selected_provider_tasks_and_self_tasks() {
        let mut db = task_db();
        update_tasks_for_service(
            &mut db,
            &"provider".parse().unwrap(),
            &"configure".parse().unwrap(),
            &json!({ "enabled": true }),
            true,
        )
        .unwrap();
        let tasks = owner_tasks(&db, "consumer");
        assert!(!tasks.contains_key(&"once".parse().unwrap()));
        assert!(!tasks.contains_key(&"conditionalOnce".parse().unwrap()));
        assert!(tasks.contains_key(&"unrelated".parse().unwrap()));
        assert!(owner_tasks(&db, "provider").is_empty());
        for owner in ["different", "unresolved"] {
            assert!(owner_tasks(&db, owner).contains_key(&"once".parse().unwrap()));
        }
    }
}
