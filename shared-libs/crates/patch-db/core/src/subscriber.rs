use std::collections::hash_map::RandomState;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::task::{ready, Poll};

use bytes::Bytes;
use futures::Stream;
use imbl_value::{InternedString, Value};
use json_patch::patch;
use json_ptr::JsonPointer;
use serde::de::DeserializeOwned;
use tokio::sync::{broadcast, mpsc};

use crate::{Dump, Error, HasModel, ModelExt, Projection, Revision};

#[cfg(test)]
mod tests;

pub type Subscriber = mpsc::UnboundedReceiver<Revision>;

/// What a subscription with a changeable projection receives, in order.
#[derive(Clone, Debug)]
pub enum Update {
    /// A revision, projected.
    Revision(Revision),
    /// What the latest projection change added, current as of its id; one per change.
    Extend(Revision),
}

/// A subscription whose projection [`PatchDb::reproject`](crate::PatchDb::reproject) changes.
#[derive(Debug)]
pub struct ProjectedSubscriber {
    pub id: u64,
    pub receiver: mpsc::UnboundedReceiver<Update>,
}

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
    projected: BTreeMap<u64, mpsc::UnboundedSender<Update>>,
    json: BTreeMap<NonZeroUsize, broadcast::Sender<JsonRevision>>,
    dump: Option<Bytes>,
}
impl ScopedBroadcast {
    fn prune_unbounded(&mut self) {
        self.unbounded.retain(|sender| !sender.is_closed());
        self.projected.retain(|_, sender| !sender.is_closed());
    }

    fn is_empty(&self) -> bool {
        self.unbounded.is_empty() && self.projected.is_empty() && self.json.is_empty()
    }
}

const HISTORY_LEN: usize = 1024;

#[derive(Debug)]
pub struct Broadcast {
    listeners: BTreeMap<(JsonPointer, Projection), ScopedBroadcast>,
    /// The listener of each subscription with a changeable projection.
    projected: BTreeMap<u64, (JsonPointer, Projection)>,
    next_projected: u64,
    history: VecDeque<Arc<Revision>>,
    epoch: InternedString,
}
impl Default for Broadcast {
    fn default() -> Self {
        Self {
            listeners: BTreeMap::new(),
            projected: BTreeMap::new(),
            next_projected: 0,
            history: VecDeque::new(),
            epoch: InternedString::from_display(&format_args!(
                "{:016x}",
                RandomState::new().build_hasher().finish()
            )),
        }
    }
}
impl Broadcast {
    pub fn new() -> Self {
        Default::default()
    }

    /// Distinguishes this broadcast's revision history from any other's.
    pub fn epoch(&self) -> &InternedString {
        &self.epoch
    }

    /// Returns the revisions after `since` up to `current`, scoped to `ptr` and projected,
    /// or `None` once the retained history no longer reaches back to `since`.
    pub fn replay(
        &self,
        ptr: &JsonPointer,
        projection: &Projection,
        since: u64,
        current: u64,
    ) -> Option<Vec<Revision>> {
        if since > current
            || since < current && self.history.front().map_or(true, |rev| rev.id > since + 1)
        {
            return None;
        }
        Some(
            self.history
                .iter()
                .filter(|rev| rev.id > since)
                .map(|rev| projection.project_revision(rev.for_path(ptr)))
                .filter(|rev| !rev.patch.is_empty())
                .collect(),
        )
    }

    pub fn send(&mut self, value: &Arc<Revision>) {
        while self.history.len() >= HISTORY_LEN {
            self.history.pop_front();
        }
        self.history.push_back(value.clone());
        self.listeners.retain(|(ptr, projection), listeners| {
            listeners.dump = None;
            listeners.prune_unbounded();
            listeners
                .json
                .retain(|_, sender| sender.receiver_count() != 0);
            if listeners.is_empty() {
                return false;
            }
            let scoped = projection.project_revision(value.for_path(ptr));
            if !scoped.patch.is_empty() {
                listeners
                    .unbounded
                    .retain(|sender| sender.send(scoped.clone()).is_ok());
                listeners.projected.retain(|_, sender| {
                    sender.send(Update::Revision(scoped.clone())).is_ok()
                });
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
        self.subscribe_projected(ptr, Projection::default())
    }

    /// Subscribers with equal pointers and projections share one listener.
    pub fn subscribe_projected(&mut self, ptr: JsonPointer, projection: Projection) -> Subscriber {
        self.listeners.retain(|_, listeners| {
            listeners.prune_unbounded();
            !listeners.is_empty()
        });
        let (send, recv) = mpsc::unbounded_channel();
        self.listeners
            .entry((ptr, projection))
            .or_default()
            .unbounded
            .push(send);
        recv
    }

    /// A subscription whose projection [`Broadcast::reproject`] changes; subscriptions holding
    /// equal projections share one listener.
    pub fn subscribe_mutable(
        &mut self,
        ptr: JsonPointer,
        projection: Projection,
    ) -> ProjectedSubscriber {
        let listeners = &self.listeners;
        self.projected.retain(|id, key| {
            listeners
                .get(key)
                .is_some_and(|l| l.projected.get(id).is_some_and(|s| !s.is_closed()))
        });
        let id = self.next_projected;
        self.next_projected += 1;
        let (send, receiver) = mpsc::unbounded_channel();
        let key = (ptr, projection);
        self.listeners
            .entry(key.clone())
            .or_default()
            .projected
            .insert(id, send);
        self.projected.insert(id, key);
        ProjectedSubscriber { id, receiver }
    }

    /// Moves a subscription to another projection, first sending it the extension of the
    /// subtree value as of the revision. False once the subscription has closed.
    pub fn reproject(&mut self, id: u64, projection: Projection, root: &Value, revision: u64) -> bool {
        let Some((ptr, held)) = self.projected.remove(&id) else {
            return false;
        };
        let Some(sender) = self
            .listeners
            .get_mut(&(ptr.clone(), held.clone()))
            .and_then(|listeners| listeners.projected.remove(&id))
        else {
            return false;
        };
        let value = ptr.get(root).unwrap_or(&Value::Null);
        let extension = Revision {
            id: revision,
            patch: held.extension(&projection, value),
        };
        if sender.send(Update::Extend(extension)).is_err() {
            return false;
        }
        let key = (ptr, projection);
        self.listeners
            .entry(key.clone())
            .or_default()
            .projected
            .insert(id, sender);
        self.projected.insert(id, key);
        true
    }

    pub fn subscribe_json(
        &mut self,
        ptr: JsonPointer,
        dump: Dump,
        capacity: NonZeroUsize,
    ) -> Result<(Bytes, JsonSubscriber), Error> {
        let listeners = self
            .listeners
            .entry((ptr.clone(), Projection::default()))
            .or_default();
        let json = match &listeners.dump {
            Some(json) => json.clone(),
            None => {
                let json = Bytes::from(serde_json::to_vec(&dump).map_err(Arc::new)?);
                listeners.dump = Some(json.clone());
                json
            }
        };
        Ok((json, self.subscribe_json_revisions(ptr, capacity)))
    }

    /// A bounded JSON revision stream without a dump, for subscribers resuming from a replay.
    pub fn subscribe_json_revisions(
        &mut self,
        ptr: JsonPointer,
        capacity: NonZeroUsize,
    ) -> JsonSubscriber {
        let receiver = self
            .listeners
            .entry((ptr, Projection::default()))
            .or_default()
            .json
            .entry(capacity)
            .or_insert_with(|| broadcast::channel(capacity.get()).0)
            .subscribe();
        JsonSubscriber {
            receiver: Some(receiver),
        }
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
