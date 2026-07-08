use std::borrow::{Borrow, Cow};
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::OsStr;
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock, Weak};

use hashbrown::raw::RawTable;
use tinyvec::ArrayVec;

#[cfg(feature = "serde")]
mod serde;

#[cfg(feature = "ts-rs")]
mod ts_rs;

#[inline]
#[cold]
fn cold() {}

const STACK_STR_SIZE: usize = 20;

enum StringRef {
    Heap(Weak<TableString>),
    Static(&'static str),
}

lazy_static::lazy_static! {
    static ref TABLE: RwLock<RawTable<StringRef>> = RwLock::new(RawTable::new());
}

#[derive(Default)]
pub struct TableHasher(xxhash_rust::xxh3::Xxh3);
impl TableHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    fn finish(&self) -> u64 {
        self.0.digest()
    }
}

struct DisplayHasher(TableHasher, Option<ArrayVec<[u8; STACK_STR_SIZE]>>);
impl DisplayHasher {
    fn finish(&self) -> (u64, Option<ArrayVec<[u8; STACK_STR_SIZE]>>) {
        (self.0.finish(), self.1)
    }
}
impl DisplayHasher {
    fn hash_and_stack<T: Display + ?Sized>(t: &T) -> (u64, Option<ArrayVec<[u8; STACK_STR_SIZE]>>) {
        use std::fmt::Write;
        let mut h = Self(TableHasher::default(), Some(ArrayVec::new()));
        let _ = write!(h, "{t}");
        h.finish()
    }
    fn hash<T: Display + ?Sized>(t: &T) -> u64 {
        use std::fmt::Write;
        let mut h = Self(TableHasher::default(), None);
        let _ = write!(h, "{t}");
        h.finish().0
    }
}
impl std::fmt::Write for DisplayHasher {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        match &mut self.1 {
            None => (),
            Some(stack) if stack.len() + s.len() <= 20 => {
                stack.extend_from_slice(s.as_bytes());
            }
            x => *x = None,
        }
        Ok(())
    }
}

struct DisplayEq<'a> {
    target: &'a [u8],
}
impl<'a> DisplayEq<'a> {
    fn eq<T: Display>(src: &T, target: &'a str) -> bool {
        use std::fmt::Write;
        let mut eq = Self {
            target: target.as_bytes(),
        };
        write!(eq, "{src}").is_ok() && eq.target.is_empty()
    }
}
impl<'a> std::fmt::Write for DisplayEq<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let s = s.as_bytes();
        if s.len() > self.target.len() || s != &self.target[..s.len()] {
            return Err(std::fmt::Error);
        }
        self.target = &self.target[s.len()..];
        Ok(())
    }
}

#[derive(Debug)]
struct TableString(String);
impl Drop for TableString {
    fn drop(&mut self) {
        let hash = DisplayHasher::hash(&self.0);
        let eq = |s: &StringRef| {
            if let StringRef::Heap(s) = s
                && s.strong_count() == 0
            {
                true
            } else {
                false
            }
        };
        let mut guard = TABLE.write().unwrap();
        if !guard.erase_entry(hash, eq) {
            cold();
            let hash = TableHasher::default().finish();
            guard.erase_entry(hash, eq);
        }
    }
}

#[derive(Debug, Clone)]
enum StringRepr {
    Heap(Arc<TableString>),
    Stack(ArrayVec<[u8; STACK_STR_SIZE]>),
    Static(&'static str),
}
impl StringRepr {
    fn as_str(&self) -> &str {
        match self {
            Self::Heap(s) => s.0.as_str(),
            Self::Stack(s) => unsafe { std::str::from_utf8_unchecked(s.as_slice()) },
            Self::Static(s) => *s,
        }
    }
}
impl PartialEq for StringRepr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Heap(a), Self::Heap(b)) => Arc::ptr_eq(a, b),
            (Self::Static(a), Self::Static(b)) => std::ptr::eq(*a, *b),
            (a, b) => a.as_str() == b.as_str(),
        }
    }
}
impl Eq for StringRepr {}
impl PartialOrd for StringRepr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self == other {
            Some(std::cmp::Ordering::Equal)
        } else {
            self.as_str().partial_cmp(other.as_str())
        }
    }
}
impl Ord for StringRepr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self == other {
            std::cmp::Ordering::Equal
        } else {
            self.as_str().cmp(other.as_str())
        }
    }
}

pub struct InternedString(StringRepr);
impl InternedString {
    pub fn intern<S: Display + Into<String>>(s: S) -> Self {
        let (hash, stack) = DisplayHasher::hash_and_stack(&s);
        if let Some(stack) = stack {
            return Self(StringRepr::Stack(stack));
        }

        // `TableString::drop` reacquires `TABLE.write()` to erase its hashbrown
        // entry. If an `Arc<TableString>` we obtained from `Weak::upgrade` is
        // the last strong reference at the moment it drops, that drop runs
        // `TableString::drop` synchronously — and if we still hold any `TABLE`
        // guard at that point, `std::sync::RwLock` self-deadlocks. To keep
        // every `eq`/rehash closure's lock-window safe, we park upgraded Arcs
        // in `keepalive` and explicitly release every TABLE guard *before*
        // dropping it at function exit. `RefCell` because hashbrown's rehash
        // closure takes `Fn` rather than `FnMut`.
        let keepalive: RefCell<Vec<Arc<TableString>>> = RefCell::new(Vec::new());
        let stash = |arc: Arc<TableString>| keepalive.borrow_mut().push(arc);

        // READ section. Compute the lookup result, explicitly release the
        // guard, then return it to the outer scope so its `Self` carries no
        // implicit guard lifetime.
        let read_hit: Option<Self> = {
            let guard = TABLE.read().unwrap();
            let result = match guard.get(hash, |ts: &StringRef| match ts {
                StringRef::Heap(ts) => match Weak::upgrade(ts) {
                    Some(arc) => {
                        let matched = DisplayEq::eq(&s, arc.0.as_str());
                        stash(arc);
                        matched
                    }
                    None => false,
                },
                StringRef::Static(ts) => DisplayEq::eq(&s, *ts),
            }) {
                Some(StringRef::Heap(ts)) => Weak::upgrade(ts).map(|a| Self(StringRepr::Heap(a))),
                Some(StringRef::Static(ts)) => Some(Self(StringRepr::Static(*ts))),
                None => None,
            };
            drop(guard);
            result
        };
        if let Some(hit) = read_hit {
            drop(keepalive);
            return hit;
        }

        // WRITE section. Same pattern: compute, explicit drop, then return.
        let written: Self = {
            let mut guard = TABLE.write().unwrap();
            // RACE CONDITION: check again under the write lock.
            let already_present: Option<Self> = match guard.get_mut(hash, |ts: &StringRef| match ts
            {
                StringRef::Heap(ts) => match Weak::upgrade(ts) {
                    Some(arc) => {
                        let matched = DisplayEq::eq(&s, arc.0.as_str());
                        stash(arc);
                        matched
                    }
                    None => false,
                },
                StringRef::Static(ts) => DisplayEq::eq(&s, *ts),
            }) {
                Some(StringRef::Heap(ts)) => {
                    cold(); // unlikely
                    Weak::upgrade(ts).map(|a| Self(StringRepr::Heap(a)))
                }
                Some(StringRef::Static(ts)) => {
                    cold(); // unlikely
                    Some(Self(StringRepr::Static(*ts)))
                }
                None => None,
            };
            let result = match already_present {
                Some(hit) => hit,
                None => {
                    let res = Arc::new(TableString(s.into()));
                    guard.insert(hash, StringRef::Heap(Arc::downgrade(&res)), |ts| {
                        let mut hasher = TableHasher::default();
                        match ts {
                            StringRef::Heap(ts) => {
                                if let Some(arc) = Weak::upgrade(ts) {
                                    hasher.write(arc.0.as_bytes());
                                    stash(arc);
                                }
                            }
                            StringRef::Static(ts) => hasher.write(ts.as_bytes()),
                        }
                        hasher.finish()
                    });
                    Self(StringRepr::Heap(res))
                }
            };
            drop(guard);
            result
        };
        drop(keepalive);
        written
    }

    pub fn from_display<S: Display + ?Sized>(s: &S) -> Self {
        struct IntoString<'a, T: ?Sized>(&'a T);
        impl<'a, T: Display + ?Sized> From<IntoString<'a, T>> for String {
            fn from(value: IntoString<'a, T>) -> Self {
                value.0.to_string()
            }
        }
        impl<'a, T: Display + ?Sized> std::fmt::Display for IntoString<'a, T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        Self::intern(IntoString(s))
    }

    pub fn from_static(s: &'static str) -> Self {
        Self(StringRepr::Static(s))
    }

    pub fn intern_static(s: &'static str) -> Self {
        let (hash, stack) = DisplayHasher::hash_and_stack(&s);
        if let Some(stack) = stack {
            return Self(StringRepr::Stack(stack));
        }
        // Same keepalive discipline as `intern` — see comment there.
        let keepalive: RefCell<Vec<Arc<TableString>>> = RefCell::new(Vec::new());
        let stash = |arc: Arc<TableString>| keepalive.borrow_mut().push(arc);

        {
            let mut guard = TABLE.write().unwrap();

            // check if it exists
            if let Some(ts) = guard.get_mut(hash, |ts: &StringRef| match ts {
                StringRef::Heap(ts) => match Weak::upgrade(ts) {
                    Some(arc) => {
                        let matched = DisplayEq::eq(&s, arc.0.as_str());
                        stash(arc);
                        matched
                    }
                    None => false,
                },
                StringRef::Static(ts) => DisplayEq::eq(&s, *ts),
            }) {
                if !matches!(ts, StringRef::Static(_)) {
                    *ts = StringRef::Static(s);
                }
            } else {
                // we need to create it
                guard.insert(hash, StringRef::Static(s), |ts| {
                    let mut hasher = TableHasher::default();
                    match ts {
                        StringRef::Heap(ts) => {
                            if let Some(arc) = Weak::upgrade(ts) {
                                hasher.write(arc.0.as_bytes());
                                stash(arc);
                            }
                        }
                        StringRef::Static(ts) => hasher.write(ts.as_bytes()),
                    }
                    hasher.finish()
                });
            }
            drop(guard);
        }
        drop(keepalive);
        Self(StringRepr::Static(s))
    }
}

impl Deref for InternedString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl AsRef<[u8]> for InternedString {
    fn as_ref(&self) -> &[u8] {
        self.0.as_str().as_ref()
    }
}

impl AsRef<OsStr> for InternedString {
    fn as_ref(&self) -> &OsStr {
        self.0.as_str().as_ref()
    }
}

impl AsRef<Path> for InternedString {
    fn as_ref(&self) -> &Path {
        self.0.as_str().as_ref()
    }
}

impl AsRef<str> for InternedString {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Borrow<str> for InternedString {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl<'a> Borrow<str> for &'a InternedString {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl Clone for InternedString {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Debug for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0.as_str(), f)
    }
}

impl Default for InternedString {
    fn default() -> Self {
        Self::intern(String::default())
    }
}

impl Display for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0.as_str(), f)
    }
}

impl PartialEq for InternedString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for InternedString {}

impl PartialOrd for InternedString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for InternedString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T: Display + Into<String>> From<T> for InternedString {
    fn from(value: T) -> Self {
        Self::intern(value)
    }
}

impl FromStr for InternedString {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::intern(s))
    }
}

impl Hash for InternedString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state)
    }
}

impl<'a> PartialEq<&'a str> for InternedString {
    fn eq(&self, other: &&'a str) -> bool {
        self.0.as_str().eq(*other)
    }
}

impl<'a> PartialEq<Cow<'a, str>> for InternedString {
    fn eq(&self, other: &Cow<'a, str>) -> bool {
        self.0.as_str().eq(other)
    }
}

impl PartialEq<str> for InternedString {
    fn eq(&self, other: &str) -> bool {
        self.0.as_str().eq(other)
    }
}

#[test]
fn test() {
    struct Suffix(&'static str, &'static str);
    impl Display for Suffix {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}{}", self.0, self.1)
        }
    }
    let left_src = Suffix("asdfasdfasdfasdf", ".asdf");
    let left = InternedString::from_display(&left_src);
    let right_src = "asdfasdfasdfasdf.asdf";
    let right = InternedString::from(right_src);
    assert_eq!(left, right);
}

#[test]
fn intern_drop_race_does_not_deadlock() {
    // Regression test for the self-deadlock that wedged a 0.4.0-beta.9 startd
    // (`startd-diag-20260523T051618Z`).
    //
    // The buggy shape: `intern()`'s `eq` closure upgrades a `Weak<TableString>`
    // to a temporary `Arc<TableString>`. If, between the upgrade and the
    // closure exiting, every other strong reference to that `TableString`
    // drops, our local Arc is the last one — and dropping it fires
    // `TableString::drop`, which re-acquires `TABLE.write()` while we still
    // hold a `TABLE.read()` (or `TABLE.write()`) guard. `std::sync::RwLock`
    // self-deadlocks; all interning threads pile up; nothing recovers.
    //
    // Reproduction recipe: many threads, few distinct keys, each thread
    // tight-loops `intern → drop`. The `strong_count` for each key flickers
    // between 1 and N constantly, so the upgrade-then-drop race fires within
    // milliseconds on the unfixed code.
    //
    // We detect the wedge two ways:
    //   1. `progress` should advance well beyond a threshold during the run.
    //   2. After signalling shutdown, all workers should join inside a watchdog
    //      window. On unfixed yasi, joining hangs because workers are parked
    //      in `TABLE.write()` reentrancy. We use `recv_timeout` on a sentinel
    //      channel rather than a literal `join()` so the test can fail rather
    //      than itself hanging.
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    const N_THREADS: usize = 32;
    const N_KEYS: usize = 4;
    const RUN: Duration = Duration::from_secs(2);
    const PROGRESS_THRESHOLD: u64 = 1_000;
    const JOIN_TIMEOUT: Duration = Duration::from_secs(30);

    // Each key must be > STACK_STR_SIZE (20) bytes so it routes through the
    // RwLock-protected path; short strings are inlined into `StringRepr::Stack`
    // and never touch TABLE.
    let keys: Vec<String> = (0..N_KEYS)
        .map(|i| format!("yasi-intern-drop-race-regression-key-{i:02}"))
        .collect();
    debug_assert!(keys.iter().all(|k| k.len() > STACK_STR_SIZE));

    let stop = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU64::new(0));

    let workers: Vec<_> = (0..N_THREADS)
        .map(|tid| {
            let stop = stop.clone();
            let progress = progress.clone();
            let key = keys[tid % N_KEYS].clone();
            thread::Builder::new()
                .name(format!("intern-stress-{tid}"))
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let s = InternedString::intern(key.clone());
                        drop(s);
                        progress.fetch_add(1, Ordering::Relaxed);
                    }
                })
                .expect("spawn")
        })
        .collect();

    let start = Instant::now();
    while start.elapsed() < RUN {
        thread::sleep(Duration::from_millis(50));
    }
    let mid_progress = progress.load(Ordering::Relaxed);
    stop.store(true, Ordering::Relaxed);

    let (tx, rx) = mpsc::channel::<()>();
    let n_workers = workers.len();
    thread::spawn(move || {
        for w in workers {
            // Worker panics propagate to the join thread but we don't need to
            // surface them — the watchdog cares about liveness only.
            let _ = w.join();
        }
        let _ = tx.send(());
    });

    match rx.recv_timeout(JOIN_TIMEOUT) {
        Ok(()) => {}
        Err(_) => panic!(
            "intern stress workers did not exit within {JOIN_TIMEOUT:?} after shutdown; \
             {n_workers} workers deadlocked. progress at shutdown = {mid_progress}, \
             threshold = {PROGRESS_THRESHOLD}",
        ),
    }

    assert!(
        mid_progress >= PROGRESS_THRESHOLD,
        "expected at least {PROGRESS_THRESHOLD} total intern/drop cycles across \
         {N_THREADS} threads in {RUN:?}; observed {mid_progress}. \
         Workers may have wedged early.",
    );
}
