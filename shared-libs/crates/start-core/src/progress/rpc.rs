use futures::{Future, Stream, StreamExt};

use super::{FullProgress, FullProgressTracker, PhasedProgressBar};
use crate::prelude::*;
use crate::util::rpc_client::RpcResponseItems;

/// Renders typed progress and returns the actual successful terminal value.
pub async fn with_cli_progress<Final: serde::de::DeserializeOwned>(
    mut items: RpcResponseItems<FullProgress, Final>,
    label: &str,
) -> Result<Final, Error> {
    let mut bar = PhasedProgressBar::new(label);
    let mut last = None;
    while let Some(snapshot) = items.next().await? {
        bar.update(&snapshot);
        last = Some(snapshot);
    }
    let result = items.finish().await?;
    if let Some(mut snapshot) = last {
        snapshot.set_complete();
        bar.update(&snapshot);
    }
    Ok(result)
}

/// Completion futures must observe an independently owned operation.
pub(crate) fn progress_until_complete(
    tracker: FullProgressTracker,
    complete: impl Future<Output = Result<(), Error>> + Send + 'static,
) -> impl Stream<Item = Result<FullProgress, Error>> + Send {
    async_stream::try_stream! {
        let mut snapshots = tracker.stream(Some(std::time::Duration::from_millis(300)));
        tokio::pin!(complete);
        let result = loop {
            tokio::select! {
                biased;
                result = &mut complete => break result,
                snapshot = snapshots.next() => {
                    yield snapshot.unwrap();
                }
            }
        };
        let mut snapshot = tracker.snapshot();
        if result.is_ok() { snapshot.set_complete(); }
        yield snapshot;
        result?;
    }
}

pub(crate) fn progress_with_null(
    source: impl Stream<Item = Result<FullProgress, Error>> + Send,
) -> impl Stream<Item = Result<Option<FullProgress>, Error>> + Send {
    async_stream::stream! {
        let mut source = Box::pin(source);
        let result = loop {
            match source.next().await {
                Some(Ok(snapshot)) => yield Ok(Some(snapshot)),
                Some(Err(error)) => break Err(error),
                None => break Ok(()),
            }
        };
        yield Ok(None);
        if let Err(error) = result { yield Err(error); }
    }
}

pub(crate) async fn forward_progress<Data: serde::Serialize>(
    mut socket: crate::rpc_continuations::WebSocket,
    source: impl Stream<Item = Result<Data, Error>> + Send,
) -> Result<(), Error> {
    let mut source = Box::pin(source);
    let result: Result<(), Error> = loop {
        tokio::select! {
            snapshot = source.next() => match snapshot {
                Some(Ok(snapshot)) => socket.send(axum::extract::ws::Message::Text(
                    serde_json::to_string(&snapshot).with_kind(ErrorKind::Serialization)?.into()
                )).await.with_kind(ErrorKind::Network)?,
                Some(Err(error)) => break Err(error),
                None => break Ok(()),
            },
            message = socket.recv() => {
                if message.transpose().with_kind(ErrorKind::Network)?.is_none() { return Ok(()); }
            }
        }
    };
    socket
        .close_result(
            result
                .as_ref()
                .map(|_| "complete")
                .map_err(ToString::to_string),
        )
        .await?;
    result
}
