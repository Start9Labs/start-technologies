use std::collections::{BTreeMap, BTreeSet};

use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::upstream::WRITES_INDEX;
use crate::rpc_continuations::Guid;
use crate::sign::AnyVerifyingKey;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

/// A key allowed to read the private s9pks that list it.
#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ReaderKey {
    pub label: String,
    pub key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(ReaderKey);
rpc_toolkit::ts_export!(ReaderKey, namespaces = [""]);

pub fn reader_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list_readers)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_readers(handle.params, result))
                .with_about("about.list-registry-readers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add_reader)
                .with_metadata(WRITES_INDEX, Value::Bool(true))
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.add-registry-reader")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "edit",
            from_fn_async(edit_reader)
                .with_metadata(WRITES_INDEX, Value::Bool(true))
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.edit-registry-reader")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_reader)
                .with_metadata(WRITES_INDEX, Value::Bool(true))
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-registry-reader")
                .with_call_remote::<CliContext>(),
        )
}

impl Model<BTreeMap<Guid, ReaderKey>> {
    pub fn ids_for(&self, key: &AnyVerifyingKey) -> Result<BTreeSet<Guid>, Error> {
        let mut ids = BTreeSet::new();
        for (id, reader) in self.as_entries()? {
            if &reader.as_key().de()? == key {
                ids.insert(id);
            }
        }
        Ok(ids)
    }

    pub fn add_reader(&mut self, reader: &ReaderKey) -> Result<Guid, Error> {
        if let Some(existing) = self.ids_for(&reader.key)?.pop_first() {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "registry.reader.already-exists",
                        id = existing,
                        key = reader.key
                    )
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        let id = Guid::new();
        self.insert(&id, reader)?;
        Ok(id)
    }

    /// Resolves each key to its entry. An unknown key fails.
    pub fn resolve_keys<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a AnyVerifyingKey>,
    ) -> Result<BTreeSet<Guid>, Error> {
        let mut ids = BTreeSet::new();
        for key in keys {
            let found = self.ids_for(key)?;
            if found.is_empty() {
                return Err(Error::new(
                    eyre!("{}", t!("registry.reader.unknown-key", key = key)),
                    ErrorKind::NotFound,
                ));
            }
            ids.extend(found);
        }
        Ok(ids)
    }
}

pub async fn list_readers(ctx: RegistryContext) -> Result<BTreeMap<Guid, ReaderKey>, Error> {
    ctx.db.peek().await.into_index().into_readers().de()
}

pub fn display_readers<T>(
    params: WithIoFormat<T>,
    readers: BTreeMap<Guid, ReaderKey>,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, readers);
    }

    let mut table = Table::new();
    table.add_row(row![bc => "ID", "LABEL", "KEY"]);
    for (id, reader) in readers {
        table.add_row(row![id.as_ref(), &reader.label, &reader.key.to_string()]);
    }
    print_table(&table)?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct AddReaderParams {
    #[arg(help = "help.arg.reader-label")]
    pub label: String,
    #[arg(help = "help.arg.reader-key", allow_hyphen_values = true)]
    pub key: AnyVerifyingKey,
}

rpc_toolkit::reflect_ts!(AddReaderParams);
rpc_toolkit::ts_export!(AddReaderParams, namespaces = [""]);

pub async fn add_reader(
    ctx: RegistryContext,
    AddReaderParams { label, key }: AddReaderParams,
) -> Result<Guid, Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_readers_mut()
                .add_reader(&ReaderKey { label, key })
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct EditReaderParams {
    #[arg(help = "help.arg.reader-id")]
    pub id: Guid,
    #[arg(long, help = "help.arg.reader-label")]
    pub label: String,
}

rpc_toolkit::reflect_ts!(EditReaderParams);
rpc_toolkit::ts_export!(EditReaderParams, namespaces = [""]);

pub async fn edit_reader(
    ctx: RegistryContext,
    EditReaderParams { id, label }: EditReaderParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_readers_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_label_mut()
                .ser(&label)
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RemoveReaderParams {
    #[arg(help = "help.arg.reader-id")]
    pub id: Guid,
}

rpc_toolkit::reflect_ts!(RemoveReaderParams);
rpc_toolkit::ts_export!(RemoveReaderParams, namespaces = [""]);

/// Also revokes the reader from every s9pk.
pub async fn remove_reader(
    ctx: RegistryContext,
    RemoveReaderParams { id }: RemoveReaderParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_readers_mut()
                .remove(&id)?
                .or_not_found(&id)?;
            for (_, package) in db
                .as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_entries_mut()?
            {
                for (_, version) in package.as_versions_mut().as_entries_mut()? {
                    version.as_s9pks_mut().mutate(|s9pks| {
                        for (_, asset) in s9pks {
                            asset.readers.remove(&id);
                        }
                        Ok(())
                    })?;
                }
            }
            Ok(())
        })
        .await
        .result
}
