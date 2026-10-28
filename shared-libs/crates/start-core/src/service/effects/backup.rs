use crate::progress::Progress;
use crate::service::effects::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetBackupProgress {
    pub progress: Progress,
}

rpc_toolkit::reflect_ts!(SetBackupProgress);
rpc_toolkit::ts_export!(SetBackupProgress, namespaces = [""]);

pub async fn set_backup_progress(
    context: EffectContext,
    SetBackupProgress { progress }: SetBackupProgress,
) -> Result<(), Error> {
    let context = context.deref()?;
    context.seed.backup_phase.mutate(|slot| {
        if let Some(handle) = slot.as_mut() {
            handle.set_phase_value(progress);
        }
    });
    Ok(())
}
