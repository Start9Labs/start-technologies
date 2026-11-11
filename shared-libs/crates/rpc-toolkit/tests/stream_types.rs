#![cfg(feature = "ts")]

mod common;

use rpc_toolkit::ts::handler_bindings;
use rpc_toolkit::{
    from_fn, from_fn_async, from_fn_async_local, from_fn_blocking, reflect_ts, Context, Empty,
    HandlerArgs, HandlerExt, ParentHandler,
};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

struct Ctx;
impl Context for Ctx {}

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Event {
    #[serde(rename(serialize = "value", deserialize = "VALUE"))]
    value: u64,
}
reflect_ts!(Event);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Summary {
    count: u64,
}
reflect_ts!(Summary);

type WatchArgs = HandlerArgs<Ctx, Empty, Empty, Event, Summary>;

fn watch(_: WatchArgs) -> Result<String, RpcError> {
    Ok("started".into())
}
async fn watch_async(args: WatchArgs) -> Result<String, RpcError> {
    watch(args)
}

#[test]
fn endpoint_stream_facts_survive_factories_adapters_and_callable_parents() {
    let tree = ParentHandler::<Ctx>::new()
        .subcommand("watch", from_fn(watch).no_cli().with_about("watch"))
        .subcommand("blocking", from_fn_blocking(watch).no_cli())
        .subcommand("async", from_fn_async(watch_async).no_cli())
        .subcommand("local", from_fn_async_local(watch_async).no_cli())
        .subcommand(
            "ordinary",
            from_fn(|_: Ctx| Ok::<_, RpcError>(true)).no_cli(),
        )
        .subcommand(
            "parent",
            ParentHandler::<Ctx>::new().root_handler(from_fn(watch).no_cli()),
        );
    let bindings = handler_bindings(&tree, "Api", common::SCHEMAS)
        .unwrap()
        .unwrap();
    common::typecheck(
        &bindings,
        r#"
      const initial: RpcReturnType<Api, 'watch'> = 'started';
      const item: RpcStreamItemType<Api, 'watch'> = {value: 1};
      const final: RpcStreamFinalType<Api, 'watch'> = {count: 2};
      const streaming: RpcIsStreaming<Api, 'watch'> = true;
      const ordinary: RpcIsStreaming<Api, 'ordinary'> = false;
      const unit: RpcStreamFinalType<Api, 'ordinary'> = null;
      const parent: RpcStreamItemType<Api, 'parent'> = item;
      const blocking: RpcStreamFinalType<Api, 'blocking'> = final;
      const asyncItem: RpcStreamItemType<Api, 'async'> = item;
      const localFinal: RpcStreamFinalType<Api, 'local'> = final;
      const method: RpcStreamingMethod<Api> = 'parent';
      // @ts-expect-error input-only field is not a streamed output
      const wrongDirection: RpcStreamItemType<Api, 'watch'> = {VALUE: 1};
      // @ts-expect-error terminal output is not an item
      const wrongFinal: RpcStreamFinalType<Api, 'watch'> = item;
      // @ts-expect-error initiating result remains distinct
      const wrongInitial: RpcReturnType<Api, 'watch'> = final;
      // @ts-expect-error non-streaming endpoints have no items
      const impossibleItem: RpcStreamItemType<Api, 'ordinary'> = 1;
      // @ts-expect-error non-streaming methods are excluded
      const wrongMethod: RpcStreamingMethod<Api> = 'ordinary';
      // @ts-expect-error a namespace without a root handler is not an endpoint
      const namespace: RpcStreamItemType<Api, ''> = item;
      // @ts-expect-error invalid paths stay never
      const invalid: RpcIsStreaming<Api, 'missing.watch'> = true;
    "#,
    );
    common::evaluate(
        &bindings,
        r#"
      const endpoint = schemas.Api._CHILDREN.watch;
      const itemSchema = z.toJSONSchema(endpoint._ITEM, {io: 'output'});
      const finalSchema = z.toJSONSchema(endpoint._FINAL, {io: 'output'});
      const item = itemSchema.$defs?.Event ?? itemSchema;
      const final = finalSchema.$defs?.Summary ?? finalSchema;
      if (!item.properties?.value || item.properties?.VALUE) throw Error('wrong item direction: ' + JSON.stringify(item));
      if (!final.properties.count) throw Error('missing terminal shape');
      if (endpoint._RETURN.parse('started') !== 'started') throw Error('lost initial result');
    "#,
    );
}

#[derive(Serialize, Deserialize)]
struct NativeReturn(u64);
fn overridden(_: HandlerArgs<Ctx, Empty, Empty, u64, bool>) -> Result<NativeReturn, RpcError> {
    Ok(NativeReturn(1))
}
struct UnboundItem;
struct UnboundFinal;
fn hidden(
    _: HandlerArgs<Ctx, Empty, Empty, UnboundItem, UnboundFinal>,
) -> Result<String, RpcError> {
    Ok("hidden".into())
}

#[test]
fn return_overrides_do_not_erase_stream_facts_or_require_original_return_bindings() {
    let tree = ParentHandler::<Ctx>::new()
        .subcommand(
            "override",
            from_fn(overridden).no_cli().override_return_ts_as::<u64>(),
        )
        .subcommand(
            "hidden",
            from_fn(hidden)
                .no_cli()
                .no_ts()
                .override_return_ts_as::<String>(),
        );
    let bindings = handler_bindings(&tree, "Api", common::SCHEMAS)
        .unwrap()
        .unwrap();
    common::typecheck(
        &bindings,
        r#"
      const initial: RpcReturnType<Api, 'override'> = 1;
      const item: RpcStreamItemType<Api, 'override'> = 2;
      const final: RpcStreamFinalType<Api, 'override'> = true;
      const streaming: RpcIsStreaming<Api, 'override'> = true;
      const hidden: RpcStreamItemType<Api, 'hidden'> = {notBound: true};
      const unknownStreaming: RpcIsStreaming<Api, 'hidden'> = false;
      // @ts-expect-error overriding initial output cannot change terminal output
      const wrongFinal: RpcStreamFinalType<Api, 'override'> = 1;
    "#,
    );
}
