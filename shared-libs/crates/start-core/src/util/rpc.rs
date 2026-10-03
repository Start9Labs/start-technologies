use std::path::Path;

use clap::Parser;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};

use crate::CAP_10_MiB;
use crate::context::CliContext;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::util::io::{ParallelBlake3Writer, open_file};
use crate::util::serde::Base16;
use crate::util::{Apply, PathOrUrl};

pub fn util<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "b3sum",
        from_fn_async(b3sum)
            .no_ts()
            .with_about("about.calculate-blake3-hash-for-file"),
    )
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct B3sumParams {
    #[arg(long = "no-mmap", action = clap::ArgAction::SetFalse, help = "help.arg.no-mmap")]
    allow_mmap: bool,
    file: String,
}

pub async fn b3sum(
    ctx: CliContext,
    B3sumParams { file, allow_mmap }: B3sumParams,
) -> Result<Base16<[u8; 32]>, Error> {
    async fn b3sum_source<S: ArchiveSource>(source: S) -> Result<Base16<[u8; 32]>, Error> {
        let mut hasher = ParallelBlake3Writer::new(CAP_10_MiB);
        source.copy_all_to(&mut hasher).await?;
        hasher.finalize().await.map(|h| *h.as_bytes()).map(Base16)
    }
    async fn b3sum_file(
        path: impl AsRef<Path>,
        allow_mmap: bool,
    ) -> Result<Base16<[u8; 32]>, Error> {
        let file = MultiCursorFile::from(open_file(path).await?);
        if allow_mmap {
            return file.blake3_mmap().await.map(|h| *h.as_bytes()).map(Base16);
        }
        b3sum_source(file).await
    }
    match file.parse::<PathOrUrl>()? {
        PathOrUrl::Path(path) => b3sum_file(path, allow_mmap).await,
        PathOrUrl::Url(url) => {
            if url.scheme() == "http" || url.scheme() == "https" {
                HttpSource::new(ctx.client.clone(), url)
                    .await?
                    .apply(b3sum_source)
                    .await
            } else {
                Err(Error::new(
                    eyre!("{}", t!("util.rpc.unknown-scheme", scheme = url.scheme())),
                    ErrorKind::InvalidRequest,
                ))
            }
        }
    }
}
