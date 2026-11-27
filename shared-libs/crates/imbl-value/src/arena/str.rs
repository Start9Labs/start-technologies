use std::alloc::Layout;
use std::borrow::Borrow;
use std::cmp::Ordering as CmpOrdering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

/// An immutable, atomically counted string stored in one value-arena allocation.
pub struct ArenaStr {
    header: NonNull<Header>,
}

unsafe impl Send for ArenaStr {}
unsafe impl Sync for ArenaStr {}

#[repr(C)]
struct Header {
    count: AtomicUsize,
    len: usize,
}

fn layout(len: usize) -> Layout {
    Layout::new::<Header>()
        .extend(Layout::array::<u8>(len).expect("string too long for the value arena"))
        .expect("string too long for the value arena")
        .0
        .pad_to_align()
}

impl ArenaStr {
    pub fn new(s: &str) -> Self {
        let header = super::alloc(layout(s.len())).cast::<Header>();
        unsafe {
            header.as_ptr().write(Header {
                count: AtomicUsize::new(1),
                len: s.len(),
            });
            std::ptr::copy_nonoverlapping(s.as_ptr(), Self::bytes(header), s.len());
        }
        Self { header }
    }

    fn bytes(header: NonNull<Header>) -> *mut u8 {
        unsafe { header.as_ptr().add(1).cast() }
    }

    fn header(&self) -> &Header {
        unsafe { self.header.as_ref() }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                Self::bytes(self.header),
                self.header().len,
            ))
        }
    }

    /// Whether both handles share one allocation.
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.header == b.header
    }
}

impl Clone for ArenaStr {
    fn clone(&self) -> Self {
        self.header().count.fetch_add(1, Ordering::Relaxed);
        Self {
            header: self.header,
        }
    }
}

impl Drop for ArenaStr {
    fn drop(&mut self) {
        if self.header().count.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }
        fence(Ordering::Acquire);
        let len = self.header().len;
        unsafe { super::dealloc(self.header.cast(), layout(len)) }
    }
}

impl Deref for ArenaStr {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for ArenaStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ArenaStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for ArenaStr {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for ArenaStr {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl From<ArenaStr> for String {
    fn from(s: ArenaStr) -> Self {
        s.as_str().to_owned()
    }
}

impl PartialEq for ArenaStr {
    fn eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other) || self.as_str() == other.as_str()
    }
}
impl Eq for ArenaStr {}

impl PartialOrd for ArenaStr {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for ArenaStr {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for ArenaStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl fmt::Debug for ArenaStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for ArenaStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}
