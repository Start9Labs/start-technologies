use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::ops::{Add, AddAssign, Bound, Range, RangeBounds};
use std::str::FromStr;

use imbl::Vector;
use imbl_value::{InOMap, InternedString, Value};
use thiserror::Error;

pub type JsonPointerRef<'a> = JsonPointer<&'a str, BorrowedSegList<'a>>;

pub const ROOT: JsonPointerRef = JsonPointer {
    src: "",
    offset: 0,
    segments: (&[], &[]),
};

#[derive(Clone, Debug, Error)]
pub enum ParseError {
    #[error("Invalid Escape: ~{0}")]
    InvalidEscape(char),
    #[error("Missing Leading \"/\"")]
    NoLeadingSlash,
}
#[derive(Clone, Debug, Error)]
pub enum IndexError {
    #[error("Could Not Index Into {0}")]
    CouldNotIndexInto(&'static str),
    #[error("Index Out Of Bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("Invalid Array Index: {0}")]
    InvalidArrayIndex(#[from] std::num::ParseIntError),
    #[error("Array Index Leading Zero")]
    ArrayIndexLeadingZero,
}

fn parse_idx(idx: &str) -> Result<usize, IndexError> {
    if idx.len() > 1 && idx.starts_with("0") {
        return Err(IndexError::ArrayIndexLeadingZero);
    }
    Ok(idx.parse()?)
}

pub type BorrowedSegList<'a> = (&'a [PtrSegment], &'a [PtrSegment]);

pub trait SegList: Sized {
    fn as_slices(&self) -> BorrowedSegList<'_>;
    fn get(&self, mut idx: usize) -> Option<&PtrSegment> {
        let slices = self.as_slices();
        for slice in [slices.0, slices.1] {
            if let Some(seg) = slice.get(idx) {
                return Some(seg);
            } else {
                idx -= slice.len();
            }
        }
        None
    }
    fn first(&self) -> Option<&PtrSegment> {
        let slices = self.as_slices();
        for slice in [slices.0, slices.1] {
            if let Some(seg) = slice.first() {
                return Some(seg);
            }
        }
        None
    }
    fn last(&self) -> Option<&PtrSegment> {
        let slices = self.as_slices();
        for slice in [slices.0, slices.1].into_iter().rev() {
            if let Some(seg) = slice.last() {
                return Some(seg);
            }
        }
        None
    }
    fn len(&self) -> usize {
        let slices = self.as_slices();
        [slices.0, slices.1]
            .into_iter()
            .fold(0, |acc, x| acc + x.len())
    }
    fn slice<R: RangeBounds<usize>>(&self, range: R) -> Option<BorrowedSegList<'_>> {
        let start_idx = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(n) => *n,
            Bound::Excluded(n) => n + 1,
        };
        let end_idx = match range.end_bound() {
            Bound::Unbounded => self.len(),
            Bound::Included(n) => n + 1,
            Bound::Excluded(n) => *n,
        };
        let (left, right) = self.as_slices();
        if start_idx <= left.len() {
            if end_idx <= left.len() {
                Some((&left[start_idx..end_idx], &[]))
            } else if end_idx - left.len() <= right.len() {
                Some((&left[start_idx..], &right[..end_idx - left.len()]))
            } else {
                None
            }
        } else if start_idx - left.len() < right.len() && end_idx - left.len() <= right.len() {
            Some((&[], &right[start_idx - left.len()..end_idx - left.len()]))
        } else {
            None
        }
    }
    fn to_vec_deque(self) -> VecDeque<PtrSegment> {
        let slices = self.as_slices();
        let mut res = VecDeque::with_capacity(self.len());
        res.extend([slices.0, slices.1].into_iter().flatten().cloned());
        res
    }
}

impl SegList for VecDeque<PtrSegment> {
    fn as_slices(&self) -> BorrowedSegList<'_> {
        self.as_slices()
    }
    fn to_vec_deque(self) -> VecDeque<PtrSegment> {
        self
    }
}

impl<'a> SegList for BorrowedSegList<'a> {
    fn as_slices(&self) -> BorrowedSegList<'_> {
        (self.0, self.1)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JsonPointer<S: AsRef<str> = String, V: SegList = VecDeque<PtrSegment>> {
    src: S,
    offset: usize,
    segments: V,
}
impl<'a, S: AsRef<str>, V: SegList> From<&'a JsonPointer<S, V>> for JsonPointerRef<'a> {
    fn from(value: &'a JsonPointer<S, V>) -> Self {
        value.borrowed()
    }
}
impl<S: AsRef<str>, V: SegList> Eq for JsonPointer<S, V> {}
impl<S: AsRef<str>, V: SegList> PartialOrd for JsonPointer<S, V> {
    fn partial_cmp(&self, other: &JsonPointer<S, V>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<S: AsRef<str>, V: SegList> Ord for JsonPointer<S, V> {
    fn cmp(&self, other: &JsonPointer<S, V>) -> Ordering {
        let mut a = self.iter();
        let mut b = other.iter();
        loop {
            let a_head = a.next();
            let b_head = b.next();
            match (a_head, b_head) {
                (None, None) => {
                    return Ordering::Equal;
                }
                (None, Some(_)) => {
                    return Ordering::Less;
                }
                (Some(_), None) => {
                    return Ordering::Greater;
                }
                (Some(p), Some(q)) => match p.cmp(q) {
                    Ordering::Equal => {
                        continue;
                    }
                    ne => {
                        return ne;
                    }
                },
            }
        }
    }
}
impl<S: AsRef<str>> JsonPointer<S> {
    pub fn parse(s: S) -> Result<Self, ParseError> {
        let src = s.as_ref();
        if src == "" {
            return Ok(JsonPointer {
                src: s,
                offset: 0,
                segments: VecDeque::new(),
            });
        }
        let mut segments = VecDeque::new();
        let mut segment = PtrSegment::Unescaped(1..1);
        let mut escape_next_char = false;
        for (idx, c) in src.char_indices() {
            if idx == 0 {
                if c == '/' {
                    continue;
                } else {
                    return Err(ParseError::NoLeadingSlash);
                }
            }
            if escape_next_char {
                match c {
                    '0' => {
                        segment = match segment {
                            PtrSegment::Unescaped(range) => PtrSegment::Escaped(
                                range.start..idx + 1,
                                src[range].to_owned() + "~",
                            ),
                            PtrSegment::Escaped(range, s) => {
                                PtrSegment::Escaped(range.start..idx + 1, s + "~")
                            }
                        }
                    }
                    '1' => {
                        segment = match segment {
                            PtrSegment::Unescaped(range) => PtrSegment::Escaped(
                                range.start..idx + 1,
                                src[range].to_owned() + "/",
                            ),
                            PtrSegment::Escaped(range, s) => {
                                PtrSegment::Escaped(range.start..idx + 1, s + "/")
                            }
                        }
                    }
                    _ => return Err(ParseError::InvalidEscape(c)),
                }
                escape_next_char = false;
            } else {
                match c {
                    '/' => {
                        segments.push_back(segment);
                        segment = PtrSegment::Unescaped(idx + 1..idx + 1);
                    }
                    '~' => {
                        escape_next_char = true;
                    }
                    _ => match segment {
                        PtrSegment::Unescaped(ref mut range) => range.end = idx + 1,
                        PtrSegment::Escaped(ref mut range, ref mut s) => {
                            range.end = idx + 1;
                            s.push(c);
                        }
                    },
                }
            }
        }
        segments.push_back(segment);
        Ok(JsonPointer {
            src: s,
            offset: 0,
            segments,
        })
    }
}
impl<S: AsRef<str>, V: SegList> JsonPointer<S, V> {
    pub fn borrowed(&self) -> JsonPointerRef<'_> {
        JsonPointer {
            src: self.src.as_ref(),
            offset: self.offset,
            segments: self.segments.as_slices(),
        }
    }
    pub fn get_segment<'a>(&'a self, idx: usize) -> Option<&'a str> {
        match self.segments.get(idx) {
            Some(PtrSegment::Unescaped(range)) => {
                Some(&self.src.as_ref()[(range.start - self.offset)..(range.end - self.offset)])
            }
            Some(PtrSegment::Escaped(_, s)) => Some(&s),
            None => None,
        }
    }
    pub fn uncons<'a>(
        &'a self,
    ) -> Option<(
        &'a str,
        JsonPointer<&'a str, (&'a [PtrSegment], &'a [PtrSegment])>,
    )> {
        if let (Some(s), Some(rest)) = (self.get_segment(0), self.slice(1..)) {
            Some((s, rest))
        } else {
            None
        }
    }
    pub fn len(&self) -> usize {
        self.segments.len()
    }
    pub fn get<'a>(&self, mut doc: &'a Value) -> Option<&'a Value> {
        for seg in self.iter() {
            doc = if doc.is_array() {
                doc.get(parse_idx(seg).ok()?)?
            } else {
                doc.get(seg)?
            };
        }
        Some(doc)
    }
    pub fn get_mut<'a>(&self, mut doc: &'a mut Value) -> Option<&'a mut Value> {
        for seg in self.iter() {
            doc = if doc.is_array() {
                doc.get_mut(parse_idx(seg).ok()?)?
            } else {
                doc.get_mut(seg)?
            };
        }
        Some(doc)
    }
    pub fn take(&self, mut doc: &mut Value) -> Option<Value> {
        for seg in self.iter() {
            doc = if doc.is_array() {
                doc.get_mut(parse_idx(seg).ok()?)?
            } else {
                doc.get_mut(seg)?
            };
        }
        Some(doc.take())
    }
    pub fn set(
        &self,
        mut doc: &mut Value,
        value: Value,
        recursive: bool,
    ) -> Result<Option<Value>, IndexError> {
        for (idx, seg) in self.iter().enumerate() {
            doc = match doc {
                Value::Array(ref mut l) => {
                    let num = if seg == "-" { l.len() } else { parse_idx(seg)? };
                    if num == l.len() {
                        if let Some(next) = self.get_segment(idx + 1) {
                            if recursive {
                                if next == "0" {
                                    l.push_back(Value::Array(Vector::new()));
                                } else {
                                    l.push_back(Value::Object(InOMap::new()))
                                }
                            }
                        } else {
                            l.push_back(value);
                            return Ok(None);
                        }
                    }
                    l.get_mut(num).ok_or(IndexError::IndexOutOfBounds(num))?
                }
                Value::Bool(_) => return Err(IndexError::CouldNotIndexInto("boolean")),
                Value::Null => return Err(IndexError::CouldNotIndexInto("null")),
                Value::Number(_) => return Err(IndexError::CouldNotIndexInto("number")),
                Value::Object(ref mut o) => {
                    if o.get(seg).is_none() {
                        if let Some(next) = self.get_segment(idx + 1) {
                            if recursive {
                                if next == "0" {
                                    o.insert(
                                        InternedString::intern(seg),
                                        Value::Array(Vector::new()),
                                    );
                                } else {
                                    o.insert(
                                        InternedString::intern(seg),
                                        Value::Object(InOMap::new()),
                                    );
                                }
                            }
                        } else {
                            o.insert(InternedString::intern(seg), value);
                            return Ok(None);
                        }
                    }
                    o.get_mut(seg).unwrap()
                }
                Value::String(_) => return Err(IndexError::CouldNotIndexInto("string")),
            }
        }
        Ok(Some(std::mem::replace(doc, value)))
    }
    pub fn insert(
        &self,
        mut doc: &mut Value,
        value: Value,
        recursive: bool,
    ) -> Result<Option<Value>, IndexError> {
        for (idx, seg) in self.iter().enumerate() {
            doc = match doc {
                Value::Array(ref mut l) => {
                    let num = if seg == "-" { l.len() } else { parse_idx(seg)? };
                    if let Some(next) = self.get_segment(idx + 1) {
                        if num == l.len() && recursive {
                            if next == "0" {
                                l.insert(num, Value::Array(Vector::new()));
                            } else {
                                l.insert(num, Value::Object(InOMap::new()))
                            }
                        }
                    } else if num <= l.len() {
                        l.insert(num, value);
                        return Ok(None);
                    }
                    l.get_mut(num).ok_or(IndexError::IndexOutOfBounds(num))?
                }
                Value::Bool(_) => return Err(IndexError::CouldNotIndexInto("boolean")),
                Value::Null => return Err(IndexError::CouldNotIndexInto("null")),
                Value::Number(_) => return Err(IndexError::CouldNotIndexInto("number")),
                Value::Object(ref mut o) => {
                    if o.get(seg).is_none() {
                        if let Some(next) = self.get_segment(idx + 1) {
                            if recursive {
                                if next == "0" {
                                    o.insert(
                                        InternedString::intern(seg),
                                        Value::Array(Vector::new()),
                                    );
                                } else {
                                    o.insert(
                                        InternedString::intern(seg),
                                        Value::Object(InOMap::new()),
                                    );
                                }
                            }
                        } else {
                            o.insert(InternedString::intern(seg), value);
                            return Ok(None);
                        }
                    }
                    o.get_mut(seg)
                        .ok_or(IndexError::CouldNotIndexInto("undefined"))?
                }
                Value::String(_) => return Err(IndexError::CouldNotIndexInto("string")),
            }
        }
        Ok(Some(std::mem::replace(doc, value)))
    }
    pub fn remove(&self, mut doc: &mut Value, allow_last: bool) -> Option<Value> {
        for (idx, seg) in self.iter().enumerate() {
            if self.get_segment(idx + 1).is_none() {
                match doc {
                    Value::Array(ref mut l) => {
                        let num = if allow_last && seg == "-" && !l.is_empty() {
                            l.len() - 1
                        } else {
                            parse_idx(seg).ok()?
                        };
                        if num < l.len() {
                            return Some(l.remove(num));
                        } else {
                            return None;
                        }
                    }
                    Value::Object(ref mut o) => {
                        return o.remove(seg);
                    }
                    _ => return None,
                }
            } else {
                doc = match doc {
                    Value::Array(ref mut arr) => {
                        if allow_last && seg == "-" && !arr.is_empty() {
                            let arr_len = arr.len();
                            arr.get_mut(arr_len - 1)?
                        } else {
                            arr.get_mut(parse_idx(seg).ok()?)?
                        }
                    }
                    Value::Object(ref mut o) => o.get_mut(seg)?,
                    _ => return None,
                };
            }
        }
        None
    }
    pub fn is_empty(&self) -> bool {
        self.segments.len() == 0
    }
    pub fn to_owned(self) -> JsonPointer {
        JsonPointer {
            src: self.src.as_ref().to_owned(),
            offset: self.offset,
            segments: self.segments.to_vec_deque(),
        }
    }
    pub fn common_prefix<'a, S0: AsRef<str>, V0: SegList>(
        &'a self,
        other: &JsonPointer<S0, V0>,
    ) -> JsonPointer<&'a str, (&'a [PtrSegment], &'a [PtrSegment])> {
        let mut common = None;
        for (idx, seg) in self.iter().enumerate() {
            if Some(seg) != other.get_segment(idx) {
                break;
            }
            common = Some(idx);
        }
        let common_idx = if let Some(common) = common {
            self.segments
                .get(common)
                .map(PtrSegment::range)
                .map(|r| r.end - self.offset)
                .unwrap_or(0)
        } else {
            0
        };
        let src = self.src.as_ref();
        JsonPointer {
            src: &src[0..common_idx],
            offset: self.offset,
            segments: self
                .segments
                .slice(0..(common.map(|a| a + 1).unwrap_or(0)))
                .unwrap(),
        }
    }
    pub fn starts_with<S0: AsRef<str>, V0: SegList>(&self, other: &JsonPointer<S0, V0>) -> bool {
        for (idx, seg) in other.iter().enumerate() {
            if self.get_segment(idx) != Some(seg) {
                return false;
            }
        }
        true
    }
    pub fn strip_prefix<'a, S0: AsRef<str>, V0: SegList>(
        &'a self,
        other: &JsonPointer<S0, V0>,
    ) -> Option<JsonPointer<&'a str, (&'a [PtrSegment], &'a [PtrSegment])>> {
        for (idx, seg) in other.iter().enumerate() {
            if self.get_segment(idx) != Some(seg) {
                return None;
            }
        }
        if self.len() == other.len() {
            return Some(Default::default());
        }
        let src_start = self.segments.get(other.segments.len())?.range().start - 1;
        Some(JsonPointer {
            src: &self.src.as_ref()[src_start..],
            offset: src_start,
            segments: self.segments.slice(other.segments.len()..)?,
        })
    }
    pub fn slice<R: RangeBounds<usize>>(
        &self,
        range: R,
    ) -> Option<JsonPointer<&str, (&[PtrSegment], &[PtrSegment])>> {
        let mut s = self.src.as_ref();
        let seg = self.segments.slice(range)?;
        let mut iter = seg.0.iter().chain(seg.1.iter());
        let offset;
        if let Some(first) = iter.next() {
            let last = iter.next_back().unwrap_or(first);
            offset = first.range().start - 1;
            s = &s[first.range().start - 1 - self.offset..last.range().end - self.offset];
        } else {
            offset = 0;
            s = "";
        }
        Some(JsonPointer {
            src: s,
            offset,
            segments: seg,
        })
    }
    pub fn iter<'a>(&'a self) -> JsonPointerIter<'a, S, V> {
        JsonPointerIter {
            ptr: self,
            start: 0,
            end: self.segments.len(),
        }
    }
}
impl<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> JsonPointer<S, V> {
    pub fn into_iter(self) -> JsonPointerIntoIter<S, V> {
        JsonPointerIntoIter {
            src: self.src,
            iter: self.segments.into_iter(),
        }
    }
}
impl JsonPointer<String> {
    pub fn push_end(&mut self, segment: &str) {
        let mut escaped = false;
        self.src.push('/');
        let start = self.src.len();
        for c in segment.chars() {
            match c {
                '~' => {
                    self.src += "~0";
                    escaped = true;
                }
                '/' => {
                    self.src += "~1";
                    escaped = true;
                }
                _ => {
                    self.src.push(c);
                }
            }
        }
        self.segments.push_back(if escaped {
            PtrSegment::Escaped(start..self.src.len(), segment.to_string())
        } else {
            PtrSegment::Unescaped(start..self.src.len())
        })
    }
    pub fn push_end_idx(&mut self, segment: usize) {
        use std::fmt::Write;
        let start = self.src.len() + 1;
        write!(self.src, "/{}", segment).unwrap();
        let end = self.src.len();
        self.segments.push_back(PtrSegment::Unescaped(start..end));
    }
    pub fn push_start(&mut self, segment: &str) {
        let escaped = segment.chars().filter(|c| *c == '~' || *c == '/').count();
        let prefix_len = segment.len() + escaped + 1;
        let mut src = String::with_capacity(self.src.len() + prefix_len);
        src.push('/');
        for c in segment.chars() {
            match c {
                '~' => {
                    src += "~0";
                }
                '/' => {
                    src += "~1";
                }
                _ => {
                    src.push(c);
                }
            }
        }
        src += self.src.as_str();
        for seg in self.segments.iter_mut() {
            let range = seg.range_mut();
            range.start += prefix_len;
            range.end += prefix_len;
        }
        self.src = src;
        self.segments.push_front(if escaped > 0 {
            PtrSegment::Escaped(1..prefix_len, segment.to_owned())
        } else {
            PtrSegment::Unescaped(1..prefix_len)
        });
    }
    pub fn push_start_idx(&mut self, segment: usize) {
        let mut src = format!("/{}", segment);
        let prefix_len = src.len();
        src += self.src.as_str();
        for seg in self.segments.iter_mut() {
            let range = seg.range_mut();
            range.start += prefix_len;
            range.end += prefix_len;
        }
        self.src = src;
        self.segments
            .insert(0, PtrSegment::Unescaped(1..prefix_len));
    }
    pub fn pop_end(&mut self) {
        if let Some(last) = self.segments.pop_back() {
            self.src.truncate(last.range().start - 1)
        }
    }
    pub fn pop_start(&mut self) {
        if let Some(last) = self.segments.pop_front() {
            let range = last.into_range();
            self.src.replace_range(range.start - 1..range.end, "");
        }
    }
    pub fn truncate(&mut self, new_len: usize) {
        if let Some(seg) = self.segments.get(new_len) {
            self.src.truncate(seg.range().start - 1);
            self.segments.truncate(new_len);
        }
    }
    pub fn join_end(mut self, segment: &str) -> Self {
        self.push_end(segment);
        self
    }
    pub fn join_end_idx(mut self, segment: usize) -> Self {
        self.push_end_idx(segment);
        self
    }
    pub fn join_start(mut self, segment: &str) -> Self {
        self.push_start(segment);
        self
    }
    pub fn join_start_idx(mut self, segment: usize) -> Self {
        self.push_start_idx(segment);
        self
    }
    pub fn append<S: AsRef<str>, V: SegList>(&mut self, suffix: &JsonPointer<S, V>) {
        for seg in suffix.iter() {
            self.push_end(seg)
        }
    }
    pub fn prepend<S: AsRef<str>, V: SegList>(&mut self, prefix: &JsonPointer<S, V>) {
        for seg in prefix.iter().rev() {
            self.push_start(seg);
        }
    }
}
impl FromStr for JsonPointer<String> {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        JsonPointer::parse(s.to_owned())
    }
}
impl<S: AsRef<str>, V: SegList> AsRef<str> for JsonPointer<S, V> {
    fn as_ref(&self) -> &str {
        self.src.as_ref()
    }
}
impl<S0, S1, V0, V1> PartialEq<JsonPointer<S1, V1>> for JsonPointer<S0, V0>
where
    S0: AsRef<str>,
    S1: AsRef<str>,
    V0: SegList,
    V1: SegList,
{
    fn eq(&self, rhs: &JsonPointer<S1, V1>) -> bool {
        self.segments.len() == rhs.segments.len() && {
            let mut rhs_iter = rhs.iter();
            self.iter().all(|lhs| Some(lhs) == rhs_iter.next())
        }
    }
}
impl<S: AsRef<str>, V: SegList> Hash for JsonPointer<S, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for seg in self.iter() {
            seg.hash(state);
        }
    }
}
impl<S: AsRef<str>, V: SegList> std::fmt::Display for JsonPointer<S, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.src.as_ref(), f)
    }
}
impl<'a, S0, S1, V0, V1> Add<&'a JsonPointer<S1, V1>> for JsonPointer<S0, V0>
where
    S0: AsRef<str> + Add<&'a str>,
    S0::Output: AsRef<str>,
    S1: AsRef<str>,
    V0: SegList + Extend<PtrSegment>,
    V1: SegList,
    for<'v> &'v V1: IntoIterator<Item = &'v PtrSegment>,
{
    type Output = JsonPointer<S0::Output, V0>;
    fn add(mut self, rhs: &'a JsonPointer<S1, V1>) -> Self::Output {
        let src_len = self.src.as_ref().len();
        let offset = self.offset;
        self.segments
            .extend((&rhs.segments).into_iter().map(|seg| match seg {
                PtrSegment::Unescaped(range) => PtrSegment::Unescaped(
                    range.start - rhs.offset + src_len + offset
                        ..range.end - rhs.offset + src_len + offset,
                ),
                PtrSegment::Escaped(range, s) => PtrSegment::Escaped(
                    range.start - rhs.offset + src_len + offset
                        ..range.end - rhs.offset + src_len + offset,
                    s.clone(),
                ),
            }));
        JsonPointer {
            src: self.src + rhs.src.as_ref(),
            offset,
            segments: self.segments,
        }
    }
}
impl<'a, S0, S1, V0, V1> Add<&'a JsonPointer<S1, V1>> for &JsonPointer<S0, V0>
where
    S0: AsRef<str> + Add<&'a str> + Clone,
    S0::Output: AsRef<str>,
    S1: AsRef<str>,
    V0: SegList + Clone + Extend<PtrSegment>,
    V1: SegList,
    for<'v> &'v V1: IntoIterator<Item = &'v PtrSegment>,
{
    type Output = JsonPointer<S0::Output, V0>;
    fn add(self, rhs: &'a JsonPointer<S1, V1>) -> Self::Output {
        let src_len = self.src.as_ref().len();
        let mut segments = self.segments.clone();
        segments.extend((&rhs.segments).into_iter().map(|seg| match seg {
            PtrSegment::Unescaped(range) => PtrSegment::Unescaped(
                range.start - rhs.offset + src_len + self.offset
                    ..range.end - rhs.offset + src_len + self.offset,
            ),
            PtrSegment::Escaped(range, s) => PtrSegment::Escaped(
                range.start - rhs.offset + src_len + self.offset
                    ..range.end - rhs.offset + src_len + self.offset,
                s.clone(),
            ),
        }));
        JsonPointer {
            src: self.src.clone() + rhs.src.as_ref(),
            offset: self.offset,
            segments,
        }
    }
}
impl<'a, S0, S1, V0, V1> AddAssign<&'a JsonPointer<S1, V1>> for JsonPointer<S0, V0>
where
    S0: AsRef<str> + AddAssign<&'a str>,
    S1: AsRef<str>,
    V0: SegList + Extend<PtrSegment>,
    V1: SegList,
    for<'v> &'v V1: IntoIterator<Item = &'v PtrSegment>,
{
    fn add_assign(&mut self, rhs: &'a JsonPointer<S1, V1>) {
        let src_len = self.src.as_ref().len();
        let offset = self.offset;
        self.segments
            .extend((&rhs.segments).into_iter().map(|seg| match seg {
                PtrSegment::Unescaped(range) => PtrSegment::Unescaped(
                    range.start - rhs.offset + src_len + offset
                        ..range.end - rhs.offset + src_len + offset,
                ),
                PtrSegment::Escaped(range, s) => PtrSegment::Escaped(
                    range.start - rhs.offset + src_len + offset
                        ..range.end - rhs.offset + src_len + offset,
                    s.clone(),
                ),
            }));
        self.src += rhs.src.as_ref();
    }
}
impl<'de, S> serde::de::Deserialize<'de> for JsonPointer<S>
where
    S: AsRef<str> + serde::de::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(JsonPointer::parse(S::deserialize(deserializer)?).map_err(serde::de::Error::custom)?)
    }
}
impl<S> serde::ser::Serialize for JsonPointer<S>
where
    S: AsRef<str> + serde::ser::Serialize,
{
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::ser::Serializer,
    {
        self.src.serialize(serializer)
    }
}

#[derive(Clone, Debug)]
pub enum PtrSegment {
    Unescaped(Range<usize>),
    Escaped(Range<usize>, String),
}
impl PtrSegment {
    fn range(&self) -> &Range<usize> {
        match self {
            PtrSegment::Unescaped(range) => range,
            PtrSegment::Escaped(range, _) => range,
        }
    }
    fn range_mut(&mut self) -> &mut Range<usize> {
        match self {
            PtrSegment::Unescaped(range) => range,
            PtrSegment::Escaped(range, _) => range,
        }
    }
    fn into_range(self) -> Range<usize> {
        match self {
            PtrSegment::Unescaped(range) => range,
            PtrSegment::Escaped(range, _) => range,
        }
    }
}

pub struct JsonPointerIter<'a, S: AsRef<str> + 'a, V: SegList> {
    ptr: &'a JsonPointer<S, V>,
    start: usize,
    end: usize,
}
impl<'a, S: AsRef<str>, V: SegList> Iterator for JsonPointerIter<'a, S, V> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            let ret = self.ptr.get_segment(self.start);
            self.start += 1;
            ret
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.end - self.start;
        (size, Some(size))
    }
}
impl<'a, S: AsRef<str>, V: SegList> DoubleEndedIterator for JsonPointerIter<'a, S, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            self.end -= 1;
            self.ptr.get_segment(self.end)
        } else {
            None
        }
    }
}

pub struct JsonPointerIntoIter<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> {
    src: S,
    iter: V::IntoIter,
}
impl<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> JsonPointerIntoIter<S, V> {
    fn next<'a>(&'a mut self) -> Option<Cow<'a, str>> {
        if let Some(seg) = self.iter.next() {
            Some(match seg {
                PtrSegment::Unescaped(range) => Cow::Borrowed(&self.src.as_ref()[range]),
                PtrSegment::Escaped(_, s) => Cow::Owned(s),
            })
        } else {
            None
        }
    }
}
impl<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> JsonPointerIntoIter<S, V>
where
    V::IntoIter: DoubleEndedIterator,
{
    fn next_back<'a>(&'a mut self) -> Option<Cow<'a, str>> {
        if let Some(seg) = self.iter.next_back() {
            Some(match seg {
                PtrSegment::Unescaped(range) => Cow::Borrowed(&self.src.as_ref()[range]),
                PtrSegment::Escaped(_, s) => Cow::Owned(s),
            })
        } else {
            None
        }
    }
}
impl<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> Iterator
    for JsonPointerIntoIter<S, V>
{
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        self.next().map(|s| s.to_string())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
impl<S: AsRef<str>, V: IntoIterator<Item = PtrSegment> + SegList> DoubleEndedIterator
    for JsonPointerIntoIter<S, V>
where
    V::IntoIter: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next_back().map(|s| s.to_string())
    }
}

#[test]
fn uncons_base() {
    let base: JsonPointer = "".parse().unwrap();
    assert_eq!(base.uncons(), None)
}

#[test]
fn uncons_inductive() {
    let inductive: JsonPointer = "/test/check".parse().unwrap();
    let (first, rest) = inductive.uncons().unwrap();
    assert_eq!(first, "test");
    assert_eq!(rest, "/check".parse::<JsonPointer>().unwrap());
}
