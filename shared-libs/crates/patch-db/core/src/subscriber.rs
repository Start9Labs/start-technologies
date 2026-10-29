use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::task::{ready, Poll};

use bytes::Bytes;
use futures::Stream;
use imbl_value::Value;
use json_patch::patch;
use json_ptr::JsonPointer;
use serde::de::DeserializeOwned;
use tokio::sync::{broadcast, mpsc};

use crate::{Dump, Error, HasModel, ModelExt, Revision};

#[cfg(test)]
mod tests;

pub type Subscriber = mpsc::UnboundedReceiver<Revision>;

type JsonRevision = Result<Bytes, Arc<serde_json::Error>>;

/// A bounded JSON revision stream. Overflow permanently closes the subscription.
///
/// Capacity is measured in revisions and may be rounded up by Tokio. After
/// `recv` returns `None`, consumers must obtain a fresh dump and subscription.
#[derive(Debug)]
pub struct JsonSubscriber {
    receiver: Option<broadcast::Receiver<JsonRevision>>,
}
impl JsonSubscriber {
    /// Returns shared JSON bytes, or `None` after closure or overflow.
    /// An encoding error also terminates the subscription.
    pub async fn recv(&mut self) -> Result<Option<Bytes>, Error> {
        let Some(receiver) = &mut self.receiver else {
            return Ok(None);
        };
        match receiver.recv().await {
            Ok(Ok(json)) => Ok(Some(json)),
            Ok(Err(error)) => {
                self.receiver = None;
                Err(error.into())
            }
            Err(_) => {
                self.receiver = None;
                Ok(None)
            }
        }
    }
}

#[derive(Debug, Default)]
struct ScopedBroadcast {
    unbounded: Vec<mpsc::UnboundedSender<Revision>>,
    json: BTreeMap<NonZeroUsize, broadcast::Sender<JsonRevision>>,
    dump: Option<Bytes>,
}
impl ScopedBroadcast {
    fn prune_unbounded(&mut self) {
        self.unbounded.retain(|sender| !sender.is_closed());
    }

    fn is_empty(&self) -> bool {
        self.unbounded.is_empty() && self.json.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct Broadcast {
    listeners: BTreeMap<JsonPointer, ScopedBroadcast>,
}
impl Broadcast {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn send(&mut self, value: &Revision) {
        self.listeners.retain(|ptr, listeners| {
            listeners.dump = None;
            listeners.prune_unbounded();
            listeners
                .json
                .retain(|_, sender| sender.receiver_count() != 0);
            if listeners.is_empty() {
                return false;
            }
            let scoped = value.for_path(ptr);
            if !scoped.patch.is_empty() {
                listeners
                    .unbounded
                    .retain(|sender| sender.send(scoped.clone()).is_ok());
                if !listeners.json.is_empty() {
                    let json = serde_json::to_vec(&scoped)
                        .map(Bytes::from)
                        .map_err(Arc::new);
                    for sender in listeners.json.values() {
                        let _ = sender.send(json.clone());
                    }
                }
            }
            true
        });
    }

    pub fn subscribe(&mut self, ptr: JsonPointer) -> Subscriber {
        self.listeners.retain(|_, listeners| {
            listeners.prune_unbounded();
            !listeners.is_empty()
        });
        let (send, recv) = mpsc::unbounded_channel();
        self.listeners.entry(ptr).or_default().unbounded.push(send);
        recv
    }

    pub fn subscribe_json(
        &mut self,
        ptr: JsonPointer,
        dump: Dump,
        capacity: NonZeroUsize,
    ) -> Result<(Bytes, JsonSubscriber), Error> {
        let listeners = self.listeners.entry(ptr).or_default();
        let json = match &listeners.dump {
            Some(json) => json.clone(),
            None => {
                let json = Bytes::from(serde_json::to_vec(&dump).map_err(Arc::new)?);
                listeners.dump = Some(json.clone());
                json
            }
        };
        let receiver = listeners
            .json
            .entry(capacity)
            .or_insert_with(|| broadcast::channel(capacity.get()).0)
            .subscribe();
        Ok((
            json,
            JsonSubscriber {
                receiver: Some(receiver),
            },
        ))
    }
}

#[derive(Debug)]
pub struct DbWatch {
    state: Value,
    subscriber: Subscriber,
    seen: bool,
}
impl DbWatch {
    pub fn new(dump: Dump, sub: Subscriber) -> Self {
        Self {
            state: dump.value,
            subscriber: sub,
            seen: false,
        }
    }
    pub fn typed<T>(self) -> TypedDbWatch<T> {
        TypedDbWatch {
            watch: self,
            _phantom: PhantomData,
        }
    }
    pub fn sync(&mut self) -> Result<(), Error> {
        while let Ok(rev) = self.subscriber.try_recv() {
            patch(&mut self.state, &rev.patch.0)?;
            self.seen = false;
        }
        Ok(())
    }
    pub fn peek(&mut self) -> Result<Value, Error> {
        self.sync()?;
        Ok(self.state.clone())
    }
    pub fn peek_and_mark_seen(&mut self) -> Result<Value, Error> {
        self.sync()?;
        self.seen = true;
        Ok(self.state.clone())
    }
    pub fn poll_changed(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Error>> {
        if !self.seen {
            self.seen = true;
            return Poll::Ready(Ok(()));
        }
        let rev =
            ready!(self.subscriber.poll_recv(cx)).ok_or(mpsc::error::TryRecvError::Disconnected)?;
        patch(&mut self.state, &rev.patch.0)?;
        while let Ok(rev) = self.subscriber.try_recv() {
            patch(&mut self.state, &rev.patch.0)?;
        }
        Poll::Ready(Ok(()))
    }
    pub async fn changed(&mut self) -> Result<(), Error> {
        futures::future::poll_fn(|cx| self.poll_changed(cx)).await
    }
}
impl Unpin for DbWatch {}
impl Stream for DbWatch {
    type Item = Result<Value, Error>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Err(e) = ready!(this.poll_changed(cx)) {
            return Poll::Ready(Some(Err(e)));
        }
        Poll::Ready(Some(Ok(this.state.clone())))
    }
}

pub struct TypedDbWatch<T> {
    watch: DbWatch,
    _phantom: PhantomData<T>,
}
impl<T> AsRef<DbWatch> for TypedDbWatch<T> {
    fn as_ref(&self) -> &DbWatch {
        &self.watch
    }
}
impl<T> AsMut<DbWatch> for TypedDbWatch<T> {
    fn as_mut(&mut self) -> &mut DbWatch {
        &mut self.watch
    }
}
impl<T> TypedDbWatch<T> {
    pub fn untyped(self) -> DbWatch {
        self.watch
    }
    pub fn sync(&mut self) -> Result<(), Error> {
        self.as_mut().sync()
    }
    pub fn poll_changed(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Error>> {
        self.as_mut().poll_changed(cx)
    }
    pub async fn changed(&mut self) -> Result<(), Error> {
        self.as_mut().changed().await
    }
}
impl<T: HasModel> TypedDbWatch<T> {
    pub fn peek(&mut self) -> Result<T::Model, Error> {
        let peek = self.as_mut().peek()?;
        Ok(<T::Model as ModelExt<T>>::from_value(peek))
    }
    pub fn peek_and_mark_seen(&mut self) -> Result<T::Model, Error> {
        let peek = self.as_mut().peek_and_mark_seen()?;
        Ok(<T::Model as ModelExt<T>>::from_value(peek))
    }
}
impl<T: DeserializeOwned> TypedDbWatch<T> {
    /// Waits until `f` accepts the current value, then yields it
    pub async fn wait_for<F: FnMut(&T) -> bool>(&mut self, mut f: F) -> Result<T, Error> {
        loop {
            let value = imbl_value::from_value(self.as_mut().peek_and_mark_seen()?)?;
            if f(&value) {
                return Ok(value);
            }
            self.changed().await?;
        }
    }
}
impl<T> Unpin for TypedDbWatch<T> {}
impl<T: HasModel> Stream for TypedDbWatch<T> {
    type Item = Result<T::Model, Error>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Err(e) = ready!(this.poll_changed(cx)) {
            return Poll::Ready(Some(Err(e)));
        }
        Poll::Ready(Some(Ok(<T::Model as ModelExt<T>>::from_value(
            this.watch.state.clone(),
        ))))
    }
}
