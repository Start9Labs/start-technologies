use std::cmp::Ordering;
use std::fmt;
use std::hash::Hash;
use std::ops::Deref;

use either::Either;
use fp_core::empty::Empty;
use fp_core::monoid::Monoid;
use fp_core::semigroup::Semigroup;
use itertools::Itertools;
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use smallvec::{smallvec, SmallVec};
use yasi::InternedString;

#[derive(Clone, Debug)]
pub enum ParseError {
    InvalidVersion(String, &'static str),
    InvalidVersionRange(String, Option<pest::error::Error<Rule>>),
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidVersion(input, reason) => {
                write!(f, "Parse Error: {input:?} is not a valid Version: {reason}")
            }
            ParseError::InvalidVersionRange(input, error) => {
                write!(f, "Parse Error: {input:?} is not a valid VersionRange")?;
                if let Some(error) = error {
                    write!(f, ": {error}")?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreReleaseSegment {
    Number(usize),
    String(InternedString),
}
impl fmt::Display for PreReleaseSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(a) => write!(f, "{a}"),
            Self::String(a) => write!(f, "{a}"),
        }
    }
}
impl From<usize> for PreReleaseSegment {
    fn from(value: usize) -> Self {
        Self::Number(value)
    }
}
impl<'a> From<&'a str> for PreReleaseSegment {
    fn from(value: &'a str) -> Self {
        Self::String(value.into())
    }
}
impl From<String> for PreReleaseSegment {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}
impl From<InternedString> for PreReleaseSegment {
    fn from(value: InternedString) -> Self {
        Self::String(value)
    }
}

#[derive(Clone, Debug)]
pub struct Version {
    number: SmallVec<[usize; 3]>, // typically 3 digits
    prerelease: Vec<PreReleaseSegment>,
}
impl Version {
    pub fn new(
        number: impl IntoIterator<Item = usize>,
        prerelease: impl IntoIterator<Item = PreReleaseSegment>,
    ) -> Self {
        Self {
            number: number.into_iter().collect(),
            prerelease: prerelease.into_iter().collect(),
        }
    }
    fn semantic_number(&self) -> &[usize] {
        let sem_len = self
            .number
            .iter()
            .enumerate()
            .rev()
            .find(|(_, seg)| **seg != 0)
            .map(|(l, _)| l + 1)
            .unwrap_or(1);
        &self.number[..sem_len]
    }

    pub fn number(&self) -> &[usize] {
        &self.number
    }

    pub fn prerelease(&self) -> &[PreReleaseSegment] {
        &self.prerelease
    }

    pub fn without_prerelease(&self) -> Self {
        Self {
            number: self.number.clone(),
            prerelease: Vec::new(),
        }
    }

    pub fn with_prerelease(
        mut self,
        prerelease: impl IntoIterator<Item = PreReleaseSegment>,
    ) -> Self {
        self.prerelease.extend(prerelease);
        self
    }

    /// Predicate for deciding whether the 'Version' is in the 'VersionRange'
    pub fn satisfies(&self, range: &VersionRange) -> bool {
        ExtendedVersion::new(self.clone(), Version::default()).satisfies(range)
    }
}
impl Default for Version {
    fn default() -> Self {
        Self {
            number: smallvec![0],
            prerelease: Vec::new(),
        }
    }
}
impl Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.semantic_number().hash(state);
        self.prerelease.hash(state);
    }
}
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.semantic_number() == other.semantic_number() && self.prerelease == other.prerelease
    }
}
impl Eq for Version {}
impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        for (l, r) in self
            .number
            .iter()
            .copied()
            .zip_longest(other.number.iter().copied())
            .map(|e| e.or_default())
        {
            match l.cmp(&r) {
                Ordering::Equal => (),
                o => return o,
            }
        }
        match self.prerelease.is_empty().cmp(&other.prerelease.is_empty()) {
            Ordering::Equal => self.prerelease.cmp(&other.prerelease),
            o => o,
        }
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.number.is_empty() {
            write!(f, "0")?;
        } else {
            for seg in itertools::Itertools::intersperse(
                self.number.iter().map(Either::Right),
                Either::Left("."),
            ) {
                write!(f, "{seg}")?;
            }
        }
        if !self.prerelease.is_empty() {
            write!(f, "-")?;
            for seg in itertools::Itertools::intersperse(
                self.prerelease.iter().map(Either::Right),
                Either::Left("."),
            ) {
                write!(f, "{seg}")?;
            }
        }
        Ok(())
    }
}
impl std::str::FromStr for Version {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err_fn = |reason| ParseError::InvalidVersion(s.to_owned(), reason);
        let (number, prerelease) = s.split_once("-").map_or((s, None), |(v, s)| (v, Some(s)));
        let number = number
            .split(".")
            .map(|v| v.parse::<usize>())
            .collect::<Result<_, _>>() // TODO: std::array::try_from_fn when stable
            .map_err(|_| err_fn("invalid numeric identifier"))?;
        let prerelease = prerelease
            .map(|s| {
                s.split(".")
                    .map(|seg| {
                        if seg.is_empty() {
                            Err(err_fn("prerelease identifier may not be empty"))
                        } else if seg.chars().all(|c| c.is_ascii_digit()) {
                            if seg.len() > 1 && seg.chars().next().unwrap() == '0' {
                                Err(err_fn(
                                    "numeric prerelease identifier may not have leading zero",
                                ))
                            } else {
                                Ok(PreReleaseSegment::Number(
                                    seg.parse()
                                        .map_err(|_| err_fn("invalid numeric identifier"))?,
                                ))
                            }
                        } else if let Some(_c) = seg
                            .chars()
                            .find(|c| !matches!(c, '0'..='9'|'a'..='z'|'A'..='Z'|'-'))
                        {
                            Err(err_fn("invalid character in prerelease identifier"))
                        } else {
                            Ok(PreReleaseSegment::String(seg.into()))
                        }
                    })
                    .collect::<Result<_, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self { number, prerelease })
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}", self))
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ExtendedVersion {
    flavor: Option<InternedString>,
    upstream: Version,
    downstream: Version,
}

impl fmt::Display for ExtendedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(flavor) = &self.flavor {
            write!(f, "#{flavor}:")?;
        }
        write!(f, "{}:{}", self.upstream, self.downstream)
    }
}
impl PartialOrd for ExtendedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.flavor == other.flavor {
            Some(match self.upstream.cmp(&other.upstream) {
                Ordering::Equal => self.downstream.cmp(&other.downstream),
                o => o,
            })
        } else {
            None
        }
    }
}
impl From<Version> for ExtendedVersion {
    fn from(upstream: Version) -> Self {
        Self {
            flavor: None,
            upstream,
            downstream: Version::default(),
        }
    }
}
impl std::str::FromStr for ExtendedVersion {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err_fn = |reason| ParseError::InvalidVersion(s.to_owned(), reason);
        let (flavor, s) = s
            .strip_prefix("#")
            .and_then(|s| s.split_once(":"))
            .map_or((None, s), |(f, s)| (Some(f), s));
        flavor
            .as_ref()
            .map(|s| {
                if s.chars().all(|c| c.is_ascii_lowercase()) {
                    Ok(())
                } else {
                    Err(err_fn("invalid flavor (must be ascii lowercase)"))
                }
            })
            .transpose()?;
        let (upstream, downstream) = s
            .split_once(":")
            .ok_or_else(|| err_fn("missing downstream version"))?;

        Ok(Self {
            flavor: flavor.map(From::from),
            upstream: upstream.parse()?,
            downstream: downstream.parse()?,
        })
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for ExtendedVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}", self))
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ExtendedVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
impl From<emver::Version> for ExtendedVersion {
    fn from(value: emver::Version) -> Self {
        Self {
            flavor: None,
            upstream: Version::new([value.major(), value.minor(), value.patch()], []),
            downstream: Version::new([value.revision()], []),
        }
    }
}

impl ExtendedVersion {
    pub const fn new(upstream: Version, downstream: Version) -> Self {
        Self {
            flavor: None,
            upstream,
            downstream,
        }
    }

    fn parse_spec(s: &str) -> Result<Self, ParseError> {
        let err_fn = |reason| ParseError::InvalidVersion(s.to_owned(), reason);
        let (flavor, s) = s
            .strip_prefix("#")
            .and_then(|s| s.split_once(":"))
            .map_or((None, s), |(f, s)| (Some(f).filter(|f| !f.is_empty()), s));
        flavor
            .as_ref()
            .map(|s| {
                if s.chars().all(|c| c.is_ascii_lowercase()) {
                    Ok(())
                } else {
                    Err(err_fn("invalid flavor (must be ascii lowercase)"))
                }
            })
            .transpose()?;
        let (upstream, downstream) = s.split_once(":").unwrap_or((s, "0"));

        Ok(Self {
            flavor: flavor.map(From::from),
            upstream: upstream.parse()?,
            downstream: downstream.parse()?,
        })
    }

    pub fn flavor(&self) -> Option<&str> {
        self.flavor.as_deref()
    }

    pub fn with_flavor(mut self, flavor: impl Into<InternedString>) -> Self {
        self.flavor = Some(flavor.into()).filter(|f| !f.is_empty());
        self
    }

    pub fn without_flavor(mut self) -> Self {
        self.flavor = None;
        self
    }

    pub fn upstream(&self) -> &Version {
        &self.upstream
    }

    pub fn upstream_mut(&mut self) -> &mut Version {
        &mut self.upstream
    }

    pub fn map_upstream(self, f: impl FnOnce(Version) -> Version) -> Self {
        Self {
            flavor: self.flavor,
            upstream: f(self.upstream),
            downstream: self.downstream,
        }
    }

    pub fn downstream(&self) -> &Version {
        &self.downstream
    }

    pub fn downstream_mut(&mut self) -> &mut Version {
        &mut self.downstream
    }

    pub fn map_downstream(self, f: impl FnOnce(Version) -> Version) -> Self {
        Self {
            flavor: self.flavor,
            upstream: self.upstream,
            downstream: f(self.downstream),
        }
    }

    /// Predicate for deciding whether the 'ExtendedVersion' is in the 'VersionRange'
    pub fn satisfies(&self, range: &VersionRange) -> bool {
        use VersionRange::*;
        match range {
            Anchor(op, v) => {
                if let Some(cmp) = self.partial_cmp(v) {
                    match op {
                        Ok(c) => &cmp == c,
                        Err(c) => &cmp != c,
                    }
                } else {
                    if op == &NEQ {
                        true
                    } else {
                        false
                    }
                }
            }
            And(a, b) => self.satisfies(a) && self.satisfies(b),
            Or(a, b) => self.satisfies(a) || self.satisfies(b),
            Not(a) => !self.satisfies(a),
            Any => true,
            None => false,
            Flavor(flavor) => &self.flavor == flavor,
        }
    }
}

// Left is inversion, Right is identity
type Invertable<T> = Result<T, T>;
pub type Operator = Invertable<Ordering>;
pub const GTE: Operator = Err(Ordering::Less);
pub const LT: Operator = Ok(Ordering::Less);
pub const NEQ: Operator = Err(Ordering::Equal);
pub const EQ: Operator = Ok(Ordering::Equal);
pub const LTE: Operator = Err(Ordering::Greater);
pub const GT: Operator = Ok(Ordering::Greater);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionRange {
    Anchor(Operator, ExtendedVersion),
    And(Box<VersionRange>, Box<VersionRange>),
    Or(Box<VersionRange>, Box<VersionRange>),
    Not(Box<VersionRange>),
    Any,
    None,
    Flavor(Option<InternedString>),
}
impl VersionRange {
    /// satisfied by any version
    pub fn any() -> Self {
        Self::Any
    }
    /// unsatisfiable
    pub fn none() -> Self {
        Self::None
    }
    /// defined in relation to a specific version
    pub fn anchor(op: Operator, version: ExtendedVersion) -> Self {
        Self::Anchor(op, version.into())
    }

    pub fn caret(version: ExtendedVersion) -> Self {
        let semantic_number = version.upstream.semantic_number();
        let major_idx = semantic_number
            .iter()
            .enumerate()
            .find(|(_, seg)| **seg != 0)
            .map(|(idx, _)| idx)
            .unwrap_or(semantic_number.len() - 1);
        let max = {
            let mut v = version.clone();
            v.upstream.number = semantic_number[..major_idx]
                .iter()
                .copied()
                .chain(semantic_number.get(major_idx).map(|n| n + 1))
                .collect();
            v.downstream = Version::default();
            v
        };
        Self::and(Self::anchor(GTE, version), Self::anchor(LT, max))
    }

    pub fn tilde(version: ExtendedVersion) -> Self {
        let semantic_number = version.upstream.semantic_number();
        let major_idx = semantic_number
            .iter()
            .enumerate()
            .find(|(_, seg)| **seg != 0)
            .map(|(idx, _)| idx)
            .unwrap_or(semantic_number.len() - 1);
        let max = {
            let mut v = version.clone();
            v.upstream.number = semantic_number[..major_idx]
                .iter()
                .chain(semantic_number.get(major_idx))
                .copied()
                .chain([semantic_number.get(major_idx + 1).unwrap_or(&0) + 1])
                .collect();
            v.downstream = Version::default();
            v
        };
        Self::and(Self::anchor(GTE, version), Self::anchor(LT, max))
    }

    /// smart constructor for And, eagerly evaluates identities and annihilators
    pub fn and(a: Self, b: Self) -> Self {
        use VersionRange::*;
        match (a, b) {
            (Any, b) => b,
            (a, Any) => a,
            (None, _) => None,
            (_, None) => None,
            (a, b) => And(Box::new(a), Box::new(b)),
        }
    }
    /// smart constructor for Or, eagerly evaluates identities and annihilators
    pub fn or(a: Self, b: Self) -> Self {
        use VersionRange::*;
        match (a, b) {
            (Any, _) => Any,
            (_, Any) => Any,
            (None, b) => b,
            (a, None) => a,
            (a, b) => Or(Box::new(a), Box::new(b)),
        }
    }

    pub fn not(a: Self) -> Self {
        use VersionRange::*;
        match a {
            Anchor(EQ, v) => Anchor(NEQ, v),
            Anchor(NEQ, v) => Anchor(EQ, v),
            And(a, b) => Or(Box::new(Self::not(*a)), Box::new(Self::not(*b))),
            Or(a, b) => And(Box::new(Self::not(*a)), Box::new(Self::not(*b))),
            Not(a) => *a,
            Any => None,
            None => Any,
            a => Not(Box::new(a)),
        }
    }

    pub fn exactly(a: ExtendedVersion) -> Self {
        Self::Anchor(EQ, a)
    }

    pub fn reduce(self) -> Self {
        use VersionRange::*;
        match self {
            And(a, b) => Self::and(*a, *b),
            Or(a, b) => Self::or(*a, *b),
            Not(a) => Self::not(*a),
            a => a,
        }
    }

    fn is_expr(&self) -> bool {
        match self {
            Self::Anchor(_, _) | Self::Any | Self::None => false,
            _ => true,
        }
    }

    fn write_with_parens(self: &Box<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_expr() {
            write!(f, "({})", self.deref())
        } else {
            write!(f, "{}", self.deref())
        }
    }

    /// Returns `true` if there exists at least one [`ExtendedVersion`] that satisfies this range.
    ///
    /// Detects emptiness that the smart `and`/`or`/`not` constructors don't fold away, including
    /// flavor contradictions (e.g. `#a && #b`) and disjoint comparison anchors of the same flavor
    /// (e.g. `>=2:0 && <1:0`).
    pub fn satisfiable(&self) -> bool {
        sat::tables_of(self).satisfiable()
    }

    /// Returns `true` if this range and `other` share at least one satisfying [`ExtendedVersion`].
    /// Equivalent to `Self::and(self.clone(), other.clone()).satisfiable()`.
    pub fn intersects(&self, other: &Self) -> bool {
        sat::Tables::and(sat::tables_of(self), sat::tables_of(other)).satisfiable()
    }
}

mod sat {
    use std::cmp::Ordering;
    use std::collections::{BTreeMap, BTreeSet};

    use yasi::InternedString;

    use super::{ExtendedVersion, Version, VersionRange, EQ, GT, GTE, LT, LTE, NEQ};

    /// Mutually-exclusive partition of the flavor space, used as table keys.
    /// `Flavor(f)` matches only flavor `f`; `FlavorNot(set)` matches anything
    /// except the flavors in `set`.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub(super) enum FlavorAtom {
        Flavor(Option<InternedString>),
        FlavorNot(BTreeSet<Option<InternedString>>),
    }

    impl FlavorAtom {
        /// Conjunction of two flavor atoms; `None` if the result is empty (contradiction).
        fn and(a: &Self, b: &Self) -> Option<Self> {
            use FlavorAtom::*;
            match (a, b) {
                (Flavor(fa), Flavor(fb)) => {
                    if fa == fb {
                        Some(Flavor(fa.clone()))
                    } else {
                        None
                    }
                }
                (Flavor(f), FlavorNot(set)) | (FlavorNot(set), Flavor(f)) => {
                    if set.contains(f) {
                        None
                    } else {
                        Some(Flavor(f.clone()))
                    }
                }
                (FlavorNot(sa), FlavorNot(sb)) => Some(FlavorNot(sa.union(sb).cloned().collect())),
            }
        }
    }

    /// A point on the version number line, with `side` resolving the inclusivity at
    /// equal upstream/downstream pairs (`-1` = "just below", `1` = "just above").
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub(super) struct Point {
        upstream: Version,
        downstream: Version,
        side: i8,
    }

    /// Truth table over the version number line for a fixed flavor atom.
    /// `points` are sorted; `values.len() == points.len() + 1` and gives the formula's
    /// truth value on each segment between consecutive points (and outside the extremes).
    #[derive(Clone, Debug)]
    pub(super) struct Table {
        points: Vec<Point>,
        values: Vec<bool>,
    }

    impl Table {
        fn from_value(v: bool) -> Self {
            Table {
                points: vec![],
                values: vec![v],
            }
        }

        fn from_split(point: Point, left: bool, right: bool) -> Self {
            Table {
                points: vec![point],
                values: vec![left, right],
            }
        }

        /// Pointwise combine two tables under a binary boolean operator, collapsing adjacent
        /// segments with equal values. Implements the merge-sort-style walk from the SDK.
        fn zip(a: &Self, b: &Self, f: impl Fn(bool, bool) -> bool) -> Self {
            let mut c = Table {
                points: vec![],
                values: vec![],
            };
            let mut i = 0usize;
            let mut j = 0usize;
            loop {
                let next = f(a.values[i], b.values[j]);
                if c.values.last().copied() == Some(next) {
                    c.points.pop();
                } else {
                    c.values.push(next);
                }
                if i == a.points.len() {
                    if j == b.points.len() {
                        return c;
                    }
                    c.points.push(b.points[j].clone());
                    j += 1;
                } else if j == b.points.len() {
                    c.points.push(a.points[i].clone());
                    i += 1;
                } else {
                    match a.points[i].cmp(&b.points[j]) {
                        Ordering::Less => {
                            c.points.push(a.points[i].clone());
                            i += 1;
                        }
                        Ordering::Greater => {
                            c.points.push(b.points[j].clone());
                            j += 1;
                        }
                        Ordering::Equal => {
                            c.points.push(a.points[i].clone());
                            i += 1;
                            j += 1;
                        }
                    }
                }
            }
        }
    }

    pub(super) enum Tables {
        True,
        False,
        Map(BTreeMap<FlavorAtom, Table>),
    }

    impl Tables {
        fn eq_flavor(flavor: &Option<InternedString>) -> Self {
            let mut m = BTreeMap::new();
            m.insert(FlavorAtom::Flavor(flavor.clone()), Table::from_value(true));
            let mut not_set = BTreeSet::new();
            not_set.insert(flavor.clone());
            m.insert(FlavorAtom::FlavorNot(not_set), Table::from_value(false));
            Tables::Map(m)
        }

        fn cmp(version: &ExtendedVersion, side: i8, left: bool, right: bool) -> Self {
            let point = Point {
                upstream: version.upstream.clone(),
                downstream: version.downstream.clone(),
                side,
            };
            let mut m = BTreeMap::new();
            m.insert(
                FlavorAtom::Flavor(version.flavor.clone()),
                Table::from_split(point, left, right),
            );
            let mut not_set = BTreeSet::new();
            not_set.insert(version.flavor.clone());
            m.insert(FlavorAtom::FlavorNot(not_set), Table::from_value(false));
            Tables::Map(m)
        }

        fn not(self) -> Self {
            match self {
                Tables::True => Tables::False,
                Tables::False => Tables::True,
                Tables::Map(mut m) => {
                    for t in m.values_mut() {
                        for v in t.values.iter_mut() {
                            *v = !*v;
                        }
                    }
                    Tables::Map(m)
                }
            }
        }

        pub(super) fn and(a: Self, b: Self) -> Self {
            match (a, b) {
                (Tables::True, x) | (x, Tables::True) => x,
                (Tables::False, _) | (_, Tables::False) => Tables::False,
                (Tables::Map(am), Tables::Map(bm)) => {
                    let mut out: BTreeMap<FlavorAtom, Table> = BTreeMap::new();
                    for (fa, ta) in &am {
                        for (fb, tb) in &bm {
                            if let Some(flavor) = FlavorAtom::and(fa, fb) {
                                let combined = Table::zip(ta, tb, |x, y| x && y);
                                match out.remove(&flavor) {
                                    Some(prev) => {
                                        let merged = Table::zip(&prev, &combined, |x, y| x || y);
                                        out.insert(flavor, merged);
                                    }
                                    None => {
                                        out.insert(flavor, combined);
                                    }
                                }
                            }
                        }
                    }
                    Tables::Map(out)
                }
            }
        }

        fn or(a: Self, b: Self) -> Self {
            match (a, b) {
                (Tables::True, _) | (_, Tables::True) => Tables::True,
                (Tables::False, x) | (x, Tables::False) => x,
                (Tables::Map(am), Tables::Map(bm)) => {
                    let mut out = am;
                    for (flavor, table) in bm {
                        match out.remove(&flavor) {
                            Some(prev) => {
                                let merged = Table::zip(&prev, &table, |x, y| x || y);
                                out.insert(flavor, merged);
                            }
                            None => {
                                out.insert(flavor, table);
                            }
                        }
                    }
                    Tables::Map(out)
                }
            }
        }

        pub(super) fn satisfiable(&self) -> bool {
            match self {
                Tables::True => true,
                Tables::False => false,
                Tables::Map(m) => m.values().any(|t| t.values.iter().any(|&v| v)),
            }
        }
    }

    pub(super) fn tables_of(range: &VersionRange) -> Tables {
        match range {
            VersionRange::Any => Tables::True,
            VersionRange::None => Tables::False,
            VersionRange::Flavor(f) => Tables::eq_flavor(f),
            VersionRange::Not(a) => tables_of(a).not(),
            VersionRange::And(a, b) => Tables::and(tables_of(a), tables_of(b)),
            VersionRange::Or(a, b) => Tables::or(tables_of(a), tables_of(b)),
            VersionRange::Anchor(op, v) => {
                let op = *op;
                if op == EQ {
                    Tables::and(
                        Tables::cmp(v, -1, false, true),
                        Tables::cmp(v, 1, true, false),
                    )
                } else if op == NEQ {
                    Tables::and(
                        Tables::cmp(v, -1, false, true),
                        Tables::cmp(v, 1, true, false),
                    )
                    .not()
                } else if op == LT {
                    Tables::cmp(v, -1, true, false)
                } else if op == LTE {
                    Tables::cmp(v, 1, true, false)
                } else if op == GT {
                    Tables::cmp(v, 1, false, true)
                } else if op == GTE {
                    Tables::cmp(v, -1, false, true)
                } else {
                    unreachable!()
                }
            }
        }
    }
}
impl Default for VersionRange {
    fn default() -> Self {
        VersionRange::Any
    }
}
impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VersionRange::*;
        match self {
            Anchor(GTE, v) => write!(f, ">={}", v),
            Anchor(LT, v) => write!(f, "<{}", v),
            Anchor(NEQ, v) => write!(f, "!={}", v),
            Anchor(EQ, v) => write!(f, "={}", v), // this is equivalent to above
            Anchor(LTE, v) => write!(f, "<={}", v),
            Anchor(GT, v) => write!(f, ">{}", v),
            And(a, b) => {
                a.write_with_parens(f)?;
                write!(f, " ")?;
                b.write_with_parens(f)
            }
            Or(a, b) => {
                a.write_with_parens(f)?;
                write!(f, " || ")?;
                b.write_with_parens(f)
            }
            Not(a) => {
                write!(f, "!")?;
                a.write_with_parens(f)
            }
            Any => write!(f, "*"),
            None => write!(f, "!"),
            Flavor(Some(flavor)) => write!(f, "#{flavor}"),
            Flavor(Option::None) => write!(f, "#"),
        }
    }
}
impl std::str::FromStr for VersionRange {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version_range(
            Grammar::parse(Rule::version_range, s)
                .map_err(|e| ParseError::InvalidVersionRange(s.into(), Some(e)))?
                .next()
                .unwrap(),
        )
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for VersionRange {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}", self))
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for VersionRange {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

pub struct AnyRange(VersionRange);
impl Semigroup for AnyRange {
    fn combine(self, other: Self) -> Self {
        AnyRange(VersionRange::or(self.0, other.0))
    }
}
impl Empty for AnyRange {
    fn empty() -> Self {
        AnyRange(VersionRange::None)
    }
}
impl Monoid for AnyRange {}

pub struct AllRange(VersionRange);
impl Semigroup for AllRange {
    fn combine(self, other: Self) -> Self {
        AllRange(VersionRange::and(self.0, other.0))
    }
}
impl Empty for AllRange {
    fn empty() -> Self {
        AllRange(VersionRange::Any)
    }
}
impl Monoid for AllRange {}

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct Grammar;

fn parse_version_range<'i>(pair: Pair<'i, Rule>) -> Result<VersionRange, ParseError> {
    let input = pair.as_span().as_str();
    let mut prev = None;
    let mut op = None::<Pair<Rule>>;
    for tok in pair.into_inner() {
        match tok.as_rule() {
            Rule::version_range_atom => {
                let atom = parse_version_range_atom(tok)?;
                if let Some(p) = prev.take() {
                    if op
                        .as_ref()
                        .map_or(false, |op| matches!(op.as_rule(), Rule::or))
                    {
                        prev = Some(VersionRange::or(p, atom));
                    } else {
                        prev = Some(VersionRange::and(p, atom));
                    }
                } else {
                    prev = Some(atom);
                }
            }
            Rule::or | Rule::and => {
                op = Some(tok);
            }
            _ => (),
        }
    }
    prev.ok_or_else(|| ParseError::InvalidVersionRange(input.to_owned(), None))
}

fn parse_version_range_atom(pair: Pair<Rule>) -> Result<VersionRange, ParseError> {
    let input = pair.as_span().as_str();
    for tok in pair.into_inner() {
        match tok.as_rule() {
            Rule::version_range => return parse_version_range(tok),
            Rule::anchor => return parse_anchor(tok),
            Rule::not => return parse_not(tok),
            Rule::any => return Ok(VersionRange::Any),
            Rule::none => return Ok(VersionRange::None),
            Rule::flavor_atom => return parse_flavor_atom(tok.as_str()),
            _ => (),
        }
    }
    Err(ParseError::InvalidVersionRange(input.to_owned(), None))
}

fn parse_anchor(pair: Pair<Rule>) -> Result<VersionRange, ParseError> {
    let input = pair.as_span().as_str();
    let err_fn = || ParseError::InvalidVersionRange(input.to_owned(), None);

    let mut op = Rule::caret;
    let mut version = None;

    for tok in pair.into_inner() {
        match tok.as_rule() {
            Rule::cmp_op => {
                op = tok.into_inner().next().ok_or_else(err_fn)?.as_rule();
            }
            Rule::version_spec => {
                version = Some(ExtendedVersion::parse_spec(tok.as_span().as_str().trim())?)
            }
            _ => (),
        }
    }

    let version = version.ok_or_else(err_fn)?;
    Ok(match op {
        Rule::caret => VersionRange::caret(version),
        Rule::tilde => VersionRange::tilde(version),
        Rule::gte => VersionRange::anchor(GTE, version),
        Rule::lt => VersionRange::anchor(LT, version),
        Rule::neq => VersionRange::anchor(NEQ, version),
        Rule::eq => VersionRange::anchor(EQ, version),
        Rule::lte => VersionRange::anchor(LTE, version),
        Rule::gt => VersionRange::anchor(GT, version),
        _ => return Err(err_fn()),
    })
}

fn parse_not(pair: Pair<Rule>) -> Result<VersionRange, ParseError> {
    let input = pair.as_span().as_str();
    for tok in pair.into_inner() {
        match tok.as_rule() {
            Rule::version_range_atom => {
                return Ok(VersionRange::not(parse_version_range_atom(tok)?))
            }
            _ => (),
        }
    }
    Err(ParseError::InvalidVersionRange(input.to_owned(), None))
}

fn parse_flavor_atom(atom: &str) -> Result<VersionRange, ParseError> {
    atom.strip_prefix("#")
        .map(|f| Some(f.into()).filter(|f: &InternedString| !f.is_empty()))
        .map(VersionRange::Flavor)
        .ok_or_else(|| ParseError::InvalidVersionRange(atom.to_owned(), None))
}
