use crate::progress::Progress;
use crate::service::effects::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetInitProgress {
    pub progress: Progress,
}

rpc_toolkit::reflect_ts!(SetInitProgress);
rpc_toolkit::ts_export!(SetInitProgress, namespaces = [""]);

pub async fn set_init_progress(
    context: EffectContext,
    SetInitProgress { progress }: SetInitProgress,
) -> Result<(), Error> {
    let context = context.deref()?;
    context.seed.init_phase.mutate(|slot| {
        if let Some(handle) = slot.as_mut() {
            handle.set_phase_value(progress);
        }
    });
    Ok(())
}
