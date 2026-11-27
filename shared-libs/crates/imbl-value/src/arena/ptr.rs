use std::alloc::Layout;
use std::fmt;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

use imbl::shared_ptr::SharedPointerKind;

/// An atomically counted shared pointer whose allocation lives in the value arena.
pub struct ArenaK {
    inner: NonNull<()>,
}

unsafe impl Send for ArenaK {}
unsafe impl Sync for ArenaK {}

#[repr(C)]
struct Inner<T> {
    count: AtomicUsize,
    value: T,
}

impl ArenaK {
    fn alloc<T>(value: T) -> Self {
        let ptr = super::alloc(Layout::new::<Inner<T>>()).cast::<Inner<T>>();
        unsafe {
            ptr.as_ptr().write(Inner {
                count: AtomicUsize::new(1),
                value,
            })
        };
        Self { inner: ptr.cast() }
    }

    unsafe fn inner<T>(&self) -> &Inner<T> {
        unsafe { self.inner.cast::<Inner<T>>().as_ref() }
    }

    /// Frees the allocation without dropping its value.
    unsafe fn free<T>(&self) {
        unsafe { super::dealloc(self.inner.cast(), Layout::new::<Inner<T>>()) }
    }
}

unsafe impl SharedPointerKind for ArenaK {
    fn new<T>(v: T) -> Self {
        Self::alloc(v)
    }

    fn from_box<T>(v: Box<T>) -> Self {
        Self::alloc(*v)
    }

    unsafe fn as_ptr<T>(&self) -> *const T {
        unsafe { &self.inner::<T>().value }
    }

    unsafe fn deref<T>(&self) -> &T {
        unsafe { &self.inner::<T>().value }
    }

    unsafe fn try_unwrap<T>(self) -> Result<T, Self> {
        let inner = unsafe { self.inner::<T>() };
        if inner
            .count
            .compare_exchange(1, 0, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(self);
        }
        let value = unsafe { std::ptr::read(&inner.value) };
        unsafe { self.free::<T>() };
        Ok(value)
    }

    unsafe fn get_mut<T>(&mut self) -> Option<&mut T> {
        if unsafe { self.inner::<T>() }.count.load(Ordering::Acquire) == 1 {
            Some(unsafe { &mut self.inner.cast::<Inner<T>>().as_mut().value })
        } else {
            None
        }
    }

    unsafe fn make_mut<T: Clone>(&mut self) -> &mut T {
        if unsafe { self.inner::<T>() }.count.load(Ordering::Acquire) != 1 {
            let copy = Self::alloc(unsafe { self.deref::<T>() }.clone());
            unsafe { self.drop::<T>() };
            *self = copy;
        }
        unsafe { &mut self.inner.cast::<Inner<T>>().as_mut().value }
    }

    unsafe fn strong_count<T>(&self) -> usize {
        unsafe { self.inner::<T>() }.count.load(Ordering::Acquire)
    }

    unsafe fn clone<T>(&self) -> Self {
        unsafe { self.inner::<T>() }
            .count
            .fetch_add(1, Ordering::Relaxed);
        Self { inner: self.inner }
    }

    unsafe fn drop<T>(&mut self) {
        if unsafe { self.inner::<T>() }
            .count
            .fetch_sub(1, Ordering::Release)
            != 1
        {
            return;
        }
        fence(Ordering::Acquire);
        unsafe {
            std::ptr::drop_in_place(&mut self.inner.cast::<Inner<T>>().as_mut().value);
            self.free::<T>();
        }
    }
}

impl fmt::Debug for ArenaK {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ArenaK")
    }
}
