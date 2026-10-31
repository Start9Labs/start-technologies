use std::io::{self, Read, Write};
use std::ops::RangeInclusive;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;

use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use sha2::digest::Output;
use sha2::Digest;

pub(crate) fn sync_batch() -> usize {
    static BATCH: OnceLock<usize> = OnceLock::new();
    *BATCH.get_or_init(|| {
        std::env::var("BACKUPFS_SYNC_BATCH")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(4096)
    })
}

pub(crate) fn sync_filesystem(file: &impl AsRawFd) -> io::Result<()> {
    if unsafe { libc::syncfs(file.as_raw_fd()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct RandReader<R: Rng>(R);
impl<R: Rng> RandReader<R> {
    pub fn new(rng: R) -> Self {
        Self(rng)
    }
}
impl<R: Rng + CryptoRng> RandReader<R> {
    pub fn new_crypto(rng: R) -> Self {
        Self(rng)
    }
}
impl<R: Rng> Read for RandReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.fill_bytes(buf);
        Ok(buf.len())
    }
}

pub struct HashIO<D: Digest, T> {
    hasher: D,
    io: T,
}
impl<D: Digest, T> HashIO<D, T> {
    pub fn new(io: T) -> Self {
        Self {
            hasher: D::new(),
            io,
        }
    }
    pub fn finalize(self) -> Output<D> {
        self.hasher.finalize()
    }
}
impl<D: Digest, T: Write> Write for HashIO<D, T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.io.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}
impl<D: Digest, T: Read> Read for HashIO<D, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.io.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

#[derive(Deserialize, Serialize)]
enum NonContinuousRange {
    Empty,
    Range(RangeInclusive<u64>),
    Split(u64, Box<Self>, Box<Self>),
}
impl Default for NonContinuousRange {
    fn default() -> Self {
        Self::Empty
    }
}
impl NonContinuousRange {
    fn is_empty(&self) -> bool {
        matches!(self, NonContinuousRange::Empty)
    }
    fn size(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::Range(range) => {
                if range.is_empty() {
                    0
                } else {
                    *range.end() - *range.start() + 1
                }
            }
            Self::Split(_, left, right) => left.size() + right.size(),
        }
    }
    fn first(&self) -> Option<u64> {
        match self {
            Self::Range(range) if !range.is_empty() => Some(*range.start()),
            Self::Split(_, left, right) => left.first().or_else(|| right.first()),
            _ => None,
        }
    }
    fn last(&self) -> Option<u64> {
        match self {
            Self::Range(range) if !range.is_empty() => Some(*range.end()),
            Self::Split(_, left, right) => right.last().or_else(|| left.last()),
            _ => None,
        }
    }
    fn remove(&mut self, id: u64) -> bool {
        let mut success = false;
        let next = match std::mem::take(self) {
            Self::Empty => Self::Empty,
            Self::Range(range) => {
                if range.contains(&id) {
                    success = true;
                    let left = *range.start()..=(id - 1);
                    let right = id + 1..=*range.end();
                    match (left.is_empty(), right.is_empty()) {
                        (true, true) => Self::Empty,
                        (true, false) => Self::Range(right),
                        (false, true) => Self::Range(left),
                        (false, false) => Self::Split(
                            id,
                            Box::new(Self::Range(left)),
                            Box::new(Self::Range(right)),
                        ),
                    }
                } else {
                    Self::Range(range)
                }
            }
            Self::Split(on, mut left, mut right) => {
                if id < on {
                    success = left.remove(id);
                } else if id > on {
                    success = right.remove(id);
                }
                match (left.is_empty(), right.is_empty()) {
                    (true, true) => Self::Empty,
                    (true, false) => *right,
                    (false, true) => *left,
                    (false, false) => Self::Split(on, left, right),
                }
            }
        };
        *self = next;
        success
    }
    fn next(&self, after: u64) -> Option<u64> {
        match self {
            Self::Empty => None,
            Self::Range(range) => {
                if *range.start() >= after {
                    Some(*range.start())
                } else if range.contains(&after) {
                    Some(after)
                } else {
                    None
                }
            }
            Self::Split(on, left, right) => {
                if after < *on {
                    left.next(after).or_else(|| right.next(after))
                } else if after > *on {
                    right.next(after)
                } else {
                    None
                }
            }
        }
    }
    fn release(&mut self, id: u64) -> bool {
        let mut success = false;
        let next = match std::mem::take(self) {
            Self::Empty => {
                success = true;
                Self::Range(id..=id)
            }
            Self::Range(range) => {
                if id < *range.start() {
                    success = true;
                    let before = *range.start() - 1;
                    if before == id {
                        Self::Range(before..=*range.end())
                    } else {
                        Self::Split(
                            before,
                            Box::new(NonContinuousRange::Range(id..=id)),
                            Box::new(Self::Range(range)),
                        )
                    }
                } else if id > *range.end() {
                    success = true;
                    let after = *range.end() + 1;
                    if after == id {
                        Self::Range(*range.start()..=after)
                    } else {
                        Self::Split(
                            after,
                            Box::new(Self::Range(range)),
                            Box::new(NonContinuousRange::Range(id..=id)),
                        )
                    }
                } else {
                    Self::Range(range)
                }
            }
            Self::Split(on, mut left, mut right) => {
                if id < on {
                    success = left.release(id);
                    Self::Split(on, left, right)
                } else if id > on {
                    success = right.release(id);
                    Self::Split(on, left, right)
                } else {
                    // id == on
                    match (left.last(), right.first()) {
                        (None, None) => Self::Empty,
                        (None, Some(_)) => {
                            success = right.release(id);
                            *right
                        }
                        (Some(_), None) => {
                            success = left.release(id);
                            *left
                        }
                        (Some(l), Some(r)) => {
                            if l + 1 == id && id == r - 1 {
                                success = left.release(id);
                                Self::try_join(*left, *right).unwrap_or_else(|_| unreachable!())
                            } else if r - id < id - l {
                                success = right.release(id);
                                Self::Split(id - 1, left, right)
                            } else {
                                success = left.release(id);
                                Self::Split(id + 1, left, right)
                            }
                        }
                    }
                }
            }
        };
        *self = next;
        success
    }
    fn try_join(left: Self, right: Self) -> Result<Self, (Self, Self)> {
        match (left, right) {
            (Self::Range(left), Self::Range(right)) if *left.end() + 1 >= *right.start() => {
                Ok(Self::Range(*left.start()..=*right.end()))
            }
            (Self::Range(left), Self::Split(on, middle, right)) => Ok(Self::Split(
                on,
                Box::new(Self::try_join(Self::Range(left), *middle)?),
                right,
            )),
            (Self::Split(on, left, middle), Self::Range(right)) => Ok(Self::Split(
                on,
                left,
                Box::new(Self::try_join(*middle, Self::Range(right))?),
            )),
            (Self::Split(l_on, left, l_middle), Self::Split(r_on, r_middle, right)) => {
                Ok(Self::Split(
                    r_on,
                    Box::new(Self::Split(
                        l_on,
                        left,
                        Box::new(Self::try_join(*l_middle, *r_middle)?),
                    )),
                    right,
                ))
            }
            (left, right) => Err((left, right)),
        }
    }
}

pub struct IdPool {
    free: NonContinuousRange,
    ctr: u64,
}
impl IdPool {
    pub fn new() -> Self {
        Self {
            free: NonContinuousRange::Range(1..=u64::MAX),
            ctr: 1,
        }
    }
    pub fn empty() -> Self {
        Self {
            free: NonContinuousRange::Empty,
            ctr: 1,
        }
    }
    pub fn free_space(&self) -> u64 {
        self.free.size()
    }
    pub fn used_space(&self) -> u64 {
        u64::MAX - 1 // 0 is not a valid id
         - self.free_space()
    }
    pub fn peek_next(&self, after: u64) -> Option<u64> {
        self.free.next(after)
    }
    pub fn next(&mut self) -> Option<u64> {
        if let Some(next) = self.free.next(self.ctr) {
            self.ctr += 1;
            if self.free.remove(next) {
                Some(next)
            } else {
                unreachable!()
            }
        } else {
            None
        }
    }
    pub fn remove(&mut self, id: u64) -> bool {
        self.free.remove(id)
    }
    pub fn release(&mut self, id: u64) -> bool {
        self.free.release(id)
    }
}
impl From<NonContinuousRange> for IdPool {
    fn from(value: NonContinuousRange) -> Self {
        Self {
            ctr: value.first().unwrap_or(1),
            free: value,
        }
    }
}
impl<'de> Deserialize<'de> for IdPool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        NonContinuousRange::deserialize(deserializer).map(Self::from)
    }
}
impl Serialize for IdPool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.free.serialize(serializer)
    }
}
