use std::fmt::{Debug, Display};
use std::task::Waker;

use futures::future::{BoxFuture, FusedFuture};
use futures::stream::FusedStream;
use futures::{Future, FutureExt, Stream, StreamExt};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::ser::Error;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

pub fn extract<T: DeserializeOwned>(value: &Value) -> Result<T, RpcError> {
    imbl_value::from_value(value.clone()).map_err(invalid_params)
}

pub fn without<T: Serialize>(value: Value, remove: &T) -> Result<Value, imbl_value::Error> {
    let to_remove = imbl_value::to_value(remove)?;
    let (Value::Object(mut value), Value::Object(to_remove)) = (value, to_remove) else {
        return Err(imbl_value::Error {
            kind: imbl_value::ErrorKind::Serialization,
            source: serde_json::Error::custom("params must be object"),
        });
    };
    for k in to_remove.keys() {
        value.remove(k);
    }
    Ok(Value::Object(value))
}

pub fn combine(v1: Value, v2: Value) -> Result<Value, imbl_value::Error> {
    let (Value::Object(mut v1), Value::Object(v2)) = (v1, v2) else {
        return Err(imbl_value::Error {
            kind: imbl_value::ErrorKind::Serialization,
            source: serde_json::Error::custom("params must be object"),
        });
    };
    for (key, value) in v2 {
        if v1.insert(key.clone(), value).is_some() {
            return Err(imbl_value::Error {
                kind: imbl_value::ErrorKind::Serialization,
                source: serde_json::Error::custom(lazy_format::lazy_format!(
                    "duplicate key: {key}"
                )),
            });
        }
    }
    Ok(Value::Object(v1))
}

pub fn invalid_params(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    }
}

pub fn invalid_request(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_REQUEST_ERROR
    }
}

pub fn parse_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::PARSE_ERROR
    }
}

pub fn internal_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    }
}

pub struct Flat<A, B>(pub A, pub B);
impl<'de, A, B> Deserialize<'de> for Flat<A, B>
where
    A: DeserializeOwned,
    B: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        let a = imbl_value::from_value(v.clone()).map_err(serde::de::Error::custom)?;
        let b = imbl_value::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Flat(a, b))
    }
}
impl<A, B> Serialize for Flat<A, B>
where
    A: Serialize,
    B: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct FlatStruct<'a, A, B> {
            #[serde(flatten)]
            a: &'a A,
            #[serde(flatten)]
            b: &'a B,
        }
        FlatStruct {
            a: &self.0,
            b: &self.1,
        }
        .serialize(serializer)
    }
}
impl<A, B> clap::CommandFactory for Flat<A, B>
where
    A: clap::CommandFactory,
    B: clap::Args,
{
    fn command() -> clap::Command {
        B::augment_args(A::command())
    }
    fn command_for_update() -> clap::Command {
        B::augment_args_for_update(A::command_for_update())
    }
}
impl<A, B> clap::FromArgMatches for Flat<A, B>
where
    A: clap::FromArgMatches,
    B: clap::FromArgMatches,
{
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        Ok(Self(
            A::from_arg_matches(matches)?,
            B::from_arg_matches(matches)?,
        ))
    }
    fn from_arg_matches_mut(matches: &mut clap::ArgMatches) -> Result<Self, clap::Error> {
        Ok(Self(
            A::from_arg_matches_mut(matches)?,
            B::from_arg_matches_mut(matches)?,
        ))
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        self.0.update_from_arg_matches(matches)?;
        self.1.update_from_arg_matches(matches)?;
        Ok(())
    }
    fn update_from_arg_matches_mut(
        &mut self,
        matches: &mut clap::ArgMatches,
    ) -> Result<(), clap::Error> {
        self.0.update_from_arg_matches_mut(matches)?;
        self.1.update_from_arg_matches_mut(matches)?;
        Ok(())
    }
}

pub fn poll_select_all<'a, T>(
    futs: &mut Vec<BoxFuture<'a, T>>,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<T> {
    let item = futs
        .iter_mut()
        .enumerate()
        .find_map(|(i, f)| match f.poll_unpin(cx) {
            std::task::Poll::Pending => None,
            std::task::Poll::Ready(e) => Some((i, e)),
        });
    match item {
        Some((idx, res)) => {
            drop(futs.swap_remove(idx));
            std::task::Poll::Ready(res)
        }
        None => std::task::Poll::Pending,
    }
}

pub struct JobRunner<'a, T> {
    wakers: Vec<Waker>,
    closed: bool,
    running: Vec<BoxFuture<'a, T>>,
}
impl<'a, T> JobRunner<'a, T> {
    pub fn new() -> Self {
        JobRunner {
            wakers: Vec::new(),
            closed: false,
            running: Vec::new(),
        }
    }
    pub async fn next_result<
        Src: Stream<Item = Fut> + Unpin,
        Fut: Future<Output = T> + Send + 'a,
    >(
        &mut self,
        job_source: &mut Src,
    ) -> Option<T> {
        let mut job_source = Some(job_source);
        loop {
            let next_job_fut = async {
                if let Some(job_source) = &mut job_source {
                    job_source.next().await
                } else {
                    futures::future::pending().await
                }
            };
            tokio::select! {
                job = next_job_fut => {
                    if let Some(job) = job {
                        self.running.push(job.boxed());
                        while let Some(waker) = self.wakers.pop() {
                            waker.wake();
                        }
                    } else {
                        job_source.take();
                        self.closed = true;
                        if self.running.is_empty() {
                            return None;
                        }
                    }
                }
                res = self.next() => {
                    return res;
                }
            }
        }
    }
}
impl<'a, T> Stream for JobRunner<'a, T> {
    type Item = T;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.running.is_empty() {
            self.wakers.push(cx.waker().clone());
            return std::task::Poll::Pending;
        }
        match poll_select_all(&mut self.running, cx) {
            std::task::Poll::Pending if self.closed && self.running.is_empty() => {
                std::task::Poll::Ready(None)
            }
            a => a.map(Some),
        }
    }
}

#[pin_project::pin_project]
pub struct StreamUntil<S, F> {
    #[pin]
    stream: S,
    #[pin]
    until: F,
    done: bool,
}
impl<S, F> StreamUntil<S, F> {
    pub fn new(stream: S, until: F) -> Self {
        Self {
            stream,
            until,
            done: false,
        }
    }
}
impl<S, F> Stream for StreamUntil<S, F>
where
    S: Stream,
    F: Future,
{
    type Item = S::Item;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        *this.done = *this.done || this.until.poll(cx).is_ready();
        if *this.done {
            std::task::Poll::Ready(None)
        } else {
            this.stream.poll_next(cx)
        }
    }
}
impl<S, F> FusedStream for StreamUntil<S, F>
where
    S: FusedStream,
    F: FusedFuture,
{
    fn is_terminated(&self) -> bool {
        self.done || self.stream.is_terminated() || self.until.is_terminated()
    }
}

pub struct PhantomData<T>(std::marker::PhantomData<T>);
impl<T> PhantomData<T> {
    pub fn new() -> Self {
        PhantomData(std::marker::PhantomData)
    }
}
impl<T> Clone for PhantomData<T> {
    fn clone(&self) -> Self {
        PhantomData::new()
    }
}
impl<T> Debug for PhantomData<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
unsafe impl<T> Send for PhantomData<T> {}
unsafe impl<T> Sync for PhantomData<T> {}
