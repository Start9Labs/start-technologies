use crate::HealthCheckId;
use crate::service::effects::prelude::*;
use crate::status::health_check::NamedHealthCheckResult;

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
