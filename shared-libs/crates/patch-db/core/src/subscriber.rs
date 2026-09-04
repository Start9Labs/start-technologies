use std::marker::PhantomData;
use std::task::{ready, Poll};

use futures::Stream;
use imbl_value::Value;
use json_patch::patch;
use json_ptr::JsonPointer;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::{Dump, Error, HasModel, ModelExt, Revision};

pub type Subscriber = mpsc::UnboundedReceiver<Revision>;

#[derive(Debug)]
struct ScopedSender(JsonPointer, mpsc::UnboundedSender<Revision>);
impl ScopedSender {
    fn send(&self, revision: &Revision) -> Result<(), mpsc::error::SendError<Revision>> {
        let scoped = revision.for_path(&self.0);
        if scoped.patch.is_empty() {
            return Ok(());
        }
        self.1.send(scoped)
    }
}

#[derive(Debug)]
pub struct Broadcast {
    listeners: Vec<ScopedSender>,
}
impl Default for Broadcast {
    fn default() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }
}
impl Broadcast {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn send(&mut self, value: &Revision) {
        let mut i = 0;
        while i < self.listeners.len() {
            if self.listeners[i].send(value).is_err() {
                self.listeners.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub fn subscribe(&mut self, ptr: JsonPointer) -> Subscriber {
        let (send, recv) = mpsc::unbounded_channel();
        self.listeners.push(ScopedSender(ptr, send));
        recv
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
