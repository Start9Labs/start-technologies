use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::system::SmtpValue;

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemSmtpParams {
    #[arg(skip)]
    callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetSystemSmtpParams);
rpc_toolkit::ts_export!(GetSystemSmtpParams, namespaces = [""]);
pub async fn get_system_smtp(
    context: EffectContext,
    GetSystemSmtpParams { callback }: GetSystemSmtpParams,
) -> Result<Option<SmtpValue>, Error> {
    let context = context.deref()?;

    let ptr = "/public/serverInfo/smtp"
        .parse()
        .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await;

    let res = imbl_value::from_value(watch.peek_and_mark_seen()?)
        .with_kind(ErrorKind::Deserialization)?;

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_system_smtp(
            watch.typed::<Option<SmtpValue>>(),
            CallbackHandler::new(&context, callback),
        );
    }

    Ok(res)
}
