use std::collections::VecDeque;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Weak};
use std::task::{Poll, Waker};

use futures::Stream;
use futures::stream::BoxStream;

#[cfg(feature = "unstable")]
lazy_static::lazy_static! {
    static ref ID_CTR: AtomicUsize = AtomicUsize::new(0);
}

#[cfg(not(feature = "unstable"))]
fn annotate_lock<F, T>(f: F, _: bool) -> T
where
    F: FnOnce() -> T,
{
    f()
}

#[cfg(feature = "unstable")]
fn annotate_lock<F, T>(f: F, id: usize, write: bool) -> T
where
    F: FnOnce() -> T,
{
    use std::collections::BTreeMap;
    std::thread_local! {
        static LOCK_CTX: std::cell::RefCell<BTreeMap<usize, Result<(), usize>>> = std::cell::RefCell::new(BTreeMap::new());
    }
    if LOCK_CTX.with_borrow_mut(|ctx| {
        let panic = if write {
            ctx.contains_key(&id)
        } else {
            ctx.get(&id).copied().unwrap_or(Err(0)).is_ok()
        };
        if !panic {
            if write {
                ctx.insert(id, Ok(()));
            } else {
                if let Err(count) = ctx.entry(id).or_insert(Err(0)) {
                    *count += 1;
                }
            }
        }
        panic
    }) {
        panic!("lock {id} is already locked on this thread");
    }
    let tracer: Option<crate::util::future::NonDetachingJoinHandle<()>> =
        if tokio::runtime::Handle::try_current().is_ok() {
            let bt = std::backtrace::Backtrace::force_capture();
            Some(
                tokio::spawn(async move {
                    use std::time::Duration;

                    tokio::time::sleep(Duration::from_secs(10)).await;
                    tracing::error!("waited on lock {id} more than 10s:\n{bt}");
                })
                .into(),
            )
        } else {
            None
        };
    let res = f();
    drop(tracer);
    LOCK_CTX.with_borrow_mut(|ctx| {
        if write {
            ctx.remove(&id);
        } else {
            if ctx
                .get_mut(&id)
                .map(|c| {
                    c.as_mut().map_err(|count| {
                        *count -= 1;
                        *count
                    }) == Err(0)
                })
                .unwrap_or(false)
            {
                ctx.remove(&id);
            }
        }
    });
    res
}

#[cfg(feature = "unstable")]
#[test]
#[should_panic]
fn test_annotate_lock() {
    annotate_lock(|| annotate_lock(|| (), 0, true), 0, true)
}

#[derive(Debug, Default)]
pub struct SyncMutex<T> {
    #[cfg(feature = "unstable")]
    id: usize,
    lock: std::sync::Mutex<T>,
}
impl<T> SyncMutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            #[cfg(feature = "unstable")]
            id: ID_CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            lock: std::sync::Mutex::new(t),
        }
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn mutate<F: FnOnce(&mut T) -> U, U>(&self, f: F) -> U {
        annotate_lock(
            || f(&mut *self.lock.lock().unwrap()),
            #[cfg(feature = "unstable")]
            self.id,
            true,
        )
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn peek<F: FnOnce(&T) -> U, U>(&self, f: F) -> U {
        annotate_lock(
            || f(&*self.lock.lock().unwrap()),
            #[cfg(feature = "unstable")]
            self.id,
            true,
        )
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn replace(&self, value: T) -> T {
        annotate_lock(
            || std::mem::replace(&mut *self.lock.lock().unwrap(), value),
            #[cfg(feature = "unstable")]
            self.id,
            true,
        )
    }
}

#[derive(Debug, Default)]
pub struct SyncRwLock<T> {
    #[cfg(feature = "unstable")]
    id: usize,
    lock: std::sync::RwLock<T>,
}
impl<T> SyncRwLock<T> {
    pub fn new(t: T) -> Self {
        Self {
            #[cfg(feature = "unstable")]
            id: ID_CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            lock: std::sync::RwLock::new(t),
        }
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn mutate<F: FnOnce(&mut T) -> U, U>(&self, f: F) -> U {
        annotate_lock(
            || f(&mut *self.lock.write().unwrap()),
            #[cfg(feature = "unstable")]
            self.id,
            true,
        )
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn peek<F: FnOnce(&T) -> U, U>(&self, f: F) -> U {
        annotate_lock(
            || f(&*self.lock.read().unwrap()),
            #[cfg(feature = "unstable")]
            self.id,
            false,
        )
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn replace(&self, value: T) -> T {
        annotate_lock(
            || std::mem::replace(&mut *self.lock.write().unwrap(), value),
            #[cfg(feature = "unstable")]
            self.id,
            true,
        )
    }
}

#[derive(Debug, Default)]
pub struct AsyncMutex<T>(tokio::sync::Mutex<T>);
impl<T> AsyncMutex<T> {
    pub fn new(t: T) -> Self {
        Self(tokio::sync::Mutex::new(t))
    }
    pub async fn mutate<F: FnOnce(&mut T) -> U, U>(&self, f: F) -> U {
        f(&mut *self.0.lock().await)
    }
    pub async fn peek<F: FnOnce(&T) -> U, U>(&self, f: F) -> U {
        f(&*self.0.lock().await)
    }
    pub async fn replace(&self, value: T) -> T {
        std::mem::replace(&mut *self.lock().await, value)
    }
}
impl<T> Deref for AsyncMutex<T> {
    type Target = tokio::sync::Mutex<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Default)]
pub struct AsyncRwLock<T>(tokio::sync::RwLock<T>);
impl<T> AsyncRwLock<T> {
    pub fn new(t: T) -> Self {
        Self(tokio::sync::RwLock::new(t))
    }
    pub async fn mutate<F: FnOnce(&mut T) -> U, U>(&self, f: F) -> U {
        f(&mut *self.0.write().await)
    }
    pub async fn peek<F: FnOnce(&T) -> U, U>(&self, f: F) -> U {
        f(&*self.0.read().await)
    }
    pub async fn replace(&self, value: T) -> T {
        std::mem::replace(&mut *self.0.write().await, value)
    }
}
impl<T> Deref for AsyncRwLock<T> {
    type Target = tokio::sync::RwLock<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct WatchShared<T> {
    version: u64,
    data: T,
    wakers: Vec<Waker>,
}
impl<T> WatchShared<T> {
    fn modified(&mut self) {
        self.version += 1;
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }
}

#[pin_project::pin_project]
pub struct Watch<T> {
    shared: Arc<SyncRwLock<WatchShared<T>>>,
    version: u64,
}
impl<T> Clone for Watch<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            version: self.version,
        }
    }
}
impl<T> Watch<T> {
    pub fn new(init: T) -> Self {
        Self {
            shared: Arc::new(SyncRwLock::new(WatchShared {
                version: 1,
                data: init,
                wakers: Vec::new(),
            })),
            version: 0,
        }
    }
    pub fn clone_unseen(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            version: 0,
        }
    }
    pub fn watcher_count(&self) -> usize {
        Arc::strong_count(&self.shared)
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn poll_changed(&mut self, cx: &mut std::task::Context<'_>) -> Poll<()> {
        self.shared.mutate(|shared| {
            if shared.version != self.version {
                self.version = shared.version;
                Poll::Ready(())
            } else {
                let waker = cx.waker();
                if !shared.wakers.iter().any(|w| w.will_wake(waker)) {
                    shared.wakers.push(waker.clone());
                }
                Poll::Pending
            }
        })
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub async fn changed(&mut self) {
        futures::future::poll_fn(|cx| self.poll_changed(cx)).await
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub async fn wait_for<F: FnMut(&T) -> bool>(&mut self, mut f: F) {
        loop {
            if self.peek(&mut f) {
                break;
            }
            self.changed().await;
        }
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn send_if_modified<F: FnOnce(&mut T) -> bool>(&self, modify: F) -> bool {
        self.shared.mutate(|shared| {
            let changed = modify(&mut shared.data);
            if changed {
                shared.modified();
            }
            changed
        })
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn send_modify<U, F: FnOnce(&mut T) -> U>(&self, modify: F) -> U {
        self.shared.mutate(|shared| {
            let res = modify(&mut shared.data);
            shared.modified();
            res
        })
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn send_replace(&self, new: T) -> T {
        self.send_modify(|a| std::mem::replace(a, new))
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn send(&self, new: T) {
        self.send_replace(new);
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn mark_changed(&self) {
        self.shared.mutate(|shared| shared.modified())
    }
    pub fn mark_unseen(&mut self) {
        self.version = 0;
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn mark_seen(&mut self) {
        self.shared.peek(|shared| {
            self.version = shared.version;
        })
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn peek<U, F: FnOnce(&T) -> U>(&self, f: F) -> U {
        self.shared.peek(|shared| f(&shared.data))
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn peek_and_mark_seen<U, F: FnOnce(&T) -> U>(&mut self, f: F) -> U {
        self.shared.peek(|shared| {
            self.version = shared.version;
            f(&shared.data)
        })
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn peek_mut<U, F: FnOnce(&mut T) -> U>(&self, f: F) -> U {
        self.shared.mutate(|shared| f(&mut shared.data))
    }
}
impl<T: Clone> Watch<T> {
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn read(&self) -> T {
        self.peek(|a| a.clone())
    }
    #[cfg_attr(feature = "unstable", inline(never))]
    pub fn read_and_mark_seen(&mut self) -> T {
        self.peek_and_mark_seen(|a| a.clone())
    }
}
impl<T: Clone> futures::Stream for Watch<T> {
    type Item = T;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.shared.mutate(|shared| {
            if shared.version != *this.version {
                *this.version = shared.version;
                Poll::Ready(Some(shared.data.clone()))
            } else {
                let waker = cx.waker();
                if !shared.wakers.iter().any(|w| w.will_wake(waker)) {
                    shared.wakers.push(waker.clone());
                }
                Poll::Pending
            }
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (1, None)
    }
}

struct DupState<T, Upstream = BoxStream<'static, T>>
where
    T: Clone,
    Upstream: Stream<Item = T> + Unpin,
{
    buffer: VecDeque<T>,
    upstream: Upstream,
    pos: usize,
    pos_refs: Vec<Weak<AtomicUsize>>,
    wakers: Vec<Waker>,
}
impl<T: Clone, Upstream: Stream<Item = T> + Unpin> DupState<T, Upstream> {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Option<T>> {
        use futures::stream::StreamExt;
        let Some(next) = futures::ready!(self.upstream.poll_next_unpin(cx)) else {
            return Poll::Ready(None);
        };
        self.pos += 1;
        self.buffer.push_back(next.clone());
        for waker in self.wakers.drain(..) {
            if !waker.will_wake(cx.waker()) {
                waker.wake();
            }
        }

        Poll::Ready(Some(next))
    }
}

pub struct DupStream<T, Upstream = BoxStream<'static, T>>
where
    T: Clone,
    Upstream: Stream<Item = T> + Unpin,
{
    state: Arc<SyncMutex<DupState<T, Upstream>>>,
    pos: Arc<AtomicUsize>,
}
impl<T: Clone, Upstream: Stream<Item = T> + Unpin> DupStream<T, Upstream> {
    pub fn new(upstream: Upstream) -> Self {
        let pos = Arc::new(AtomicUsize::new(0));
        Self {
            state: Arc::new(SyncMutex::new(DupState {
                buffer: VecDeque::new(),
                upstream,
                pos: 0,
                pos_refs: vec![Arc::downgrade(&pos)],
                wakers: Vec::new(),
            })),
            pos,
        }
    }
}

impl<T: Clone, Upstream: Stream<Item = T> + Unpin> Clone for DupStream<T, Upstream> {
    fn clone(&self) -> Self {
        let pos = self.state.mutate(|state| {
            let pos = Arc::new(AtomicUsize::new(
                self.pos.load(std::sync::atomic::Ordering::Relaxed),
            ));
            state.pos_refs.push(Arc::downgrade(&pos));
            state.pos_refs.retain(|ptr| ptr.strong_count() > 0);
            pos
        });
        Self {
            state: self.state.clone(),
            pos,
        }
    }
}

impl<T: Clone, Upstream: Stream<Item = T> + Unpin> Stream for DupStream<T, Upstream> {
    type Item = T;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.state.mutate(|state| {
            let pos = self.pos.load(std::sync::atomic::Ordering::Relaxed);
            if pos < state.pos {
                self.pos.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if state
                    .pos_refs
                    .iter()
                    .filter_map(|ptr| ptr.upgrade())
                    .all(|ptr| ptr.load(std::sync::atomic::Ordering::Relaxed) > pos)
                {
                    while let Some(next) = state.buffer.pop_front() {
                        if state.buffer.len() + 1 == state.pos - pos {
                            return Poll::Ready(Some(next));
                        }
                    }
                    Poll::Ready(None)
                } else {
                    Poll::Ready(
                        state
                            .buffer
                            .get(state.buffer.len() + pos - state.pos)
                            .cloned(),
                    )
                }
            } else {
                let res = state.poll_next(cx);
                if res.is_ready() {
                    self.pos
                        .store(state.pos, std::sync::atomic::Ordering::Relaxed);
                } else {
                    let waker = cx.waker();
                    if state.wakers.iter().all(|w| !w.will_wake(waker)) {
                        state.wakers.push(waker.clone());
                    }
                }
                res
            }
        })
    }
}

#[tokio::test]
async fn test_dup_stream() {
    use std::time::Duration;

    use futures::StreamExt;

    let stream = async_stream::stream! {
        for i in 0..100 {
            tokio::time::sleep(Duration::from_nanos(rand::random_range(0..=10000000))).await;
            yield i;
        }
    }
    .boxed();
    let n = rand::random_range(3..10);
    let mut tasks = Vec::with_capacity(n);
    for mut dup_stream in std::iter::repeat_n(DupStream::new(stream), n) {
        tasks.push(tokio::spawn(async move {
            let mut ctr = 0;
            while let Some(i) = dup_stream.next().await {
                assert_eq!(ctr, i);
                ctr += 1;
                tokio::time::sleep(Duration::from_nanos(rand::random_range(0..=10000000))).await;
            }
            assert_eq!(ctr, 100);
        }));
    }
    futures::future::try_join_all(tasks).await.unwrap();
}
