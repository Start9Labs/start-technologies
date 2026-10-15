use std::collections::BTreeMap;

use clap::{ArgAction, Parser};
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::admin::display_signers;
use crate::registry::context::RegistryContext;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

pub fn access_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "get",
            from_fn_async(get_package_access)
                .with_metadata("admin", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_package_access(handle.params, result)
                })
                .with_about("about.get-package-access")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-private",
            from_fn_async(set_package_private)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.set-package-private")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add-reader",
            from_fn_async(add_package_reader)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.add-package-reader")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove-reader",
            from_fn_async(remove_package_reader)
                .with_metadata("admin", Value::Bool(true))
                .no_display()
                .with_about("about.remove-package-reader")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct GetPackageAccessParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

rpc_toolkit::reflect_ts!(GetPackageAccessParams);
rpc_toolkit::ts_export!(GetPackageAccessParams, namespaces = [""]);

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct PackageAccess {
    pub private: bool,
    pub readers: BTreeMap<Guid, SignerInfo>,
}

rpc_toolkit::reflect_ts!(PackageAccess);
rpc_toolkit::ts_export!(PackageAccess, namespaces = [""]);

pub async fn get_package_access(
    ctx: RegistryContext,
    GetPackageAccessParams { id }: GetPackageAccessParams,
) -> Result<PackageAccess, Error> {
    let db = ctx.db.peek().await;
    let package = db
        .as_index()
        .as_package()
        .as_packages()
        .as_idx(&id)
        .or_not_found(&id)?;
    Ok(PackageAccess {
        private: package.as_private().de()?,
        readers: package
            .as_readers()
            .de()?
            .into_iter()
            .filter_map(|guid| {
                db.as_index()
                    .as_signers()
                    .as_idx(&guid)
                    .map(|s| s.de().map(|s| (guid, s)))
            })
            .collect::<Result<_, _>>()?,
    })
}

fn display_package_access(
    params: WithIoFormat<GetPackageAccessParams>,
    access: PackageAccess,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, access);
    }
    println!("PRIVATE: {}", access.private);
    display_signers(params, access.readers)
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct SetPackagePrivateParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(action = ArgAction::Set, help = "help.arg.package-private")]
    pub private: bool,
}

rpc_toolkit::reflect_ts!(SetPackagePrivateParams);
rpc_toolkit::ts_export!(SetPackagePrivateParams, namespaces = [""]);

/// Creates the package entry when absent.
pub async fn set_package_private(
    ctx: RegistryContext,
    SetPackagePrivateParams { id, private }: SetPackagePrivateParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .upsert(&id, || Ok(Default::default()))?
                .as_private_mut()
                .ser(&private)
        })
        .await
        .result
}

#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct PackageReaderParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(help = "help.arg.signer-id")]
    pub signer: Guid,
}

rpc_toolkit::reflect_ts!(PackageReaderParams);
rpc_toolkit::ts_export!(PackageReaderParams, namespaces = [""]);

pub async fn add_package_reader(
    ctx: RegistryContext,
    PackageReaderParams { id, signer }: PackageReaderParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            if !db.as_index().as_signers().contains_key(&signer)? {
                return Err(Error::new(
                    eyre!("{}", t!("registry.admin.unknown-signer")),
                    ErrorKind::NotFound,
                ));
            }
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_readers_mut()
                .mutate(|readers| {
                    readers.insert(signer);
                    Ok(())
                })
        })
        .await
        .result
}

pub async fn remove_package_reader(
    ctx: RegistryContext,
    PackageReaderParams { id, signer }: PackageReaderParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let removed = db
                .as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_readers_mut()
                .mutate(|readers| Ok(readers.remove(&signer)))?;
            if !removed {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "registry.package.access.not-reader",
                            signer = signer,
                            id = id
                        )
                    ),
                    ErrorKind::NotFound,
                ));
            }
            Ok(())
        })
        .await
        .result
}
