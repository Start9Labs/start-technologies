use std::path::PathBuf;

use clap::Parser;
use itertools::Itertools;
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::context::CliContext;
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::add::can_publish;
use crate::registry::package::index::PackageIndex;
use crate::s9pk::S9pk;
use crate::s9pk::v2::SIG_CONTEXT;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::{AnySignature, AnyVerifyingKey};
use crate::util::VersionString;

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SignPackageParams {
    pub id: PackageId,
    pub version: VersionString,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    pub signer: AnyVerifyingKey,
    pub commitment: MerkleArchiveCommitment,
    pub signature: AnySignature,
}

rpc_toolkit::reflect_ts!(SignPackageParams);
rpc_toolkit::ts_export!(SignPackageParams, namespaces = [""]);

pub async fn sign_package(
    ctx: RegistryContext,
    SignPackageParams {
        id,
        version,
        signer,
        commitment,
        signature,
    }: SignPackageParams,
) -> Result<(), Error> {
    let signature =
        crate::sign::compat::verify_commitment(&signer, &commitment, SIG_CONTEXT, &signature)?;
    ctx.db
        .mutate(|db| {
            let guid = db.as_index().as_signers().get_signer(&signer)?;
            if !can_publish(db, &guid, &id, &version)? {
                return Err(Error::new(
                    eyre!("{}", t!("registry.package.sign.unauthorized")),
                    ErrorKind::Authorization,
                ));
            }
            let mut found = false;
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .as_idx_mut(&id)
                .and_then(|p| p.as_versions_mut().as_idx_mut(&version))
                .or_not_found(&format!("{id}@{version}"))?
                .as_s9pks_mut()
                .mutate(|s9pks| {
                    for (_, asset) in s9pks.iter_mut().filter(|(_, a)| a.commitment == commitment) {
                        asset.signatures.insert(signer.clone(), signature.clone());
                        found = true;
                    }
                    Ok(())
                })?;
            if !found {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "registry.package.sign.commitment-not-found",
                            id = id,
                            version = version,
                            sighash = commitment.root_sighash
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

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[command(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct CliSignPackageParams {
    #[arg(
        required_unless_present = "no_verify",
        conflicts_with = "no_verify",
        help = "help.arg.s9pk-file-path"
    )]
    pub file: Option<PathBuf>,
    #[arg(long, requires_all = ["id", "version"], help = "help.arg.no-verify-sign")]
    pub no_verify: bool,
    #[arg(long, requires = "no_verify", help = "help.arg.package-id")]
    pub id: Option<PackageId>,
    #[arg(long, requires = "no_verify", help = "help.arg.package-version")]
    pub version: Option<VersionString>,
}

pub async fn cli_sign_package(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: CliSignPackageParams {
            file, id, version, ..
        },
        ..
    }: HandlerArgs<CliContext, CliSignPackageParams>,
) -> Result<(), Error> {
    let (id, version, commitments) = if let Some(file) = file {
        let s9pk = S9pk::open(&file, None).await?;
        let manifest = s9pk.as_manifest();
        (
            manifest.id.clone(),
            manifest.version.clone(),
            vec![s9pk.as_archive().commitment().await?],
        )
    } else {
        let (Some(id), Some(version)) = (id, version) else {
            unreachable!("clap requires --id and --version with --no-verify")
        };
        let mut index: PackageIndex = from_value(
            ctx.call_remote::<RegistryContext>("package.index", imbl_value::json!({}))
                .await?,
        )?;
        let commitments = index
            .packages
            .remove(&id)
            .and_then(|mut p| p.versions.remove(&version))
            .or_not_found(&format!("{id}@{version}"))?
            .s9pks
            .into_iter()
            .map(|(_, asset)| asset.commitment)
            .unique_by(|c| c.root_sighash)
            .collect();
        (id, version, commitments)
    };

    let method = parent_method.into_iter().chain(method).join(".");
    for commitment in commitments {
        let signature = ctx
            .id_key()?
            .sign_commitment(&commitment, SIG_CONTEXT)
            .await?;
        ctx.call_remote::<RegistryContext>(
            &method,
            imbl_value::json!({
                "id": id,
                "version": version,
                "commitment": commitment,
                "signature": signature,
            }),
        )
        .await?;
    }

    Ok(())
}
