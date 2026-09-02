use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use yasi::InternedString;

use crate::exver::*;

prop_compose! {
    fn flavor_gen()(
        has_flavor in any::<bool>(),
        flavor in "[a-z]{1,25}"
    ) -> Option<InternedString> {
        if has_flavor {
            Some(flavor.into())
        } else {
            None
        }
    }
}

prop_compose! {
    fn prerelease_string()(
        string in "[a-zA-Z0-9-]{1,25}"
    ) -> PreReleaseSegment {
        if string.chars().all(|c| c.is_ascii_digit()) {
            string.parse().map_or_else(
                |_| PreReleaseSegment::BigNumber(string.into()),
                PreReleaseSegment::Number,
            )
        } else {
            PreReleaseSegment::String(string.into())
        }
    }
}

prop_compose! {
    fn big_prerelease_number()(
        string in "[1-9][0-9]{40,64}"
    ) -> PreReleaseSegment {
        PreReleaseSegment::BigNumber(string.into())
    }
}

prop_compose! {
    fn version_gen()(
        number in prop::collection::vec(any::<usize>(), 1..10),
        prerelease in prop::collection::vec(prop_oneof![
            any::<usize>().prop_map(PreReleaseSegment::Number),
            big_prerelease_number(),
            prerelease_string(),
        ], 0..3),
    ) -> Version {
        Version::new(number, prerelease)
    }
}

fn version_pair_gen() -> impl Strategy<Value = (Version, Version)> {
    (version_gen(), version_gen(), any::<bool>(), 1usize..4).prop_map(
        |(left, right, make_equal, trailing_zeros)| {
            if make_equal {
                let right = Version::new(
                    left.number()
                        .iter()
                        .copied()
                        .chain(std::iter::repeat(0).take(trailing_zeros)),
                    left.prerelease().iter().cloned(),
                );
                (left, right)
            } else {
                (left, right)
            }
        },
    )
}

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

prop_compose! {
    fn ex_version_gen()(
        flavor in flavor_gen(),
        upstream in version_gen(),
        downstream in version_gen(),
    ) -> ExtendedVersion {
        let v = ExtendedVersion::new(upstream, downstream);
        if let Some(flavor) = flavor {
            v.with_flavor(flavor)
        } else {
            v
        }
    }
}

prop_compose! {
    fn anchor_gen()(op in prop_oneof![Just(LT), Just(LTE), Just(EQ), Just(NEQ), Just(GT), Just(GTE)], v in ex_version_gen()) -> VersionRange {
        VersionRange::anchor(op, v)
    }
}

prop_compose! {
    fn and_gen(inner: impl Strategy<Value = VersionRange> + Clone)(a in inner.clone(), b in inner) -> VersionRange {
        VersionRange::and(a, b)
    }
}

prop_compose! {
    fn or_gen(inner: impl Strategy<Value = VersionRange> + Clone)(a in inner.clone(), b in inner) -> VersionRange {
        VersionRange::or(a,b)
    }
}

prop_compose! {
    fn not_gen(inner: impl Strategy<Value = VersionRange> + Clone)(a in inner) -> VersionRange {
        VersionRange::not(a)
    }
}

fn range_gen() -> BoxedStrategy<VersionRange> {
    let leaf = prop_oneof![
        Just(VersionRange::Any),
        Just(VersionRange::None),
        anchor_gen()
    ];
    leaf.prop_recursive(2, 8, 4, |inner| {
        prop_oneof![
            and_gen(inner.clone()),
            or_gen(inner.clone()),
            not_gen(inner)
        ]
    })
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig {
        fork: true,
        timeout: 1000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn and_assoc(a in range_gen(), b in range_gen(), c in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::and(a.clone(), VersionRange::and(b.clone(),c.clone()))) == obs.satisfies(&VersionRange::and(VersionRange::and(a,b),c)))
    }

    #[test]
    fn and_commut(a in range_gen(), b in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::and(a.clone(),b.clone())) == obs.satisfies(&VersionRange::and(b, a)))
    }

    #[test]
    fn or_assoc(a in range_gen(), b in range_gen(), c in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::or(a.clone(), VersionRange::or(b.clone(), c.clone()))) == obs.satisfies(&VersionRange::or(VersionRange::or(a, b), c)))
    }

    #[test]
    fn or_commut(a in range_gen(), b in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::or(a.clone(), b.clone())) == obs.satisfies(&VersionRange::or(b.clone(), a.clone())))
    }

    #[test]
    fn any_ident_and(a in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&a) == obs.satisfies(&VersionRange::and(VersionRange::Any, a)))
    }

    #[test]
    fn none_ident_or(a in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&a) == obs.satisfies(&VersionRange::or(VersionRange::None, a)))
    }

    #[test]
    fn none_annihilates_and(a in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::and(VersionRange::None, a)) == obs.satisfies(&VersionRange::None))
    }

    #[test]
    fn any_annihilates_or(a in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::or(VersionRange::Any, a)) == obs.satisfies(&VersionRange::Any))
    }

    #[test]
    fn and_distributes_over_or(a in range_gen(), b in range_gen(), c in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::and(a.clone(), VersionRange::or(b.clone(),c.clone()))) == obs.satisfies(&VersionRange::or(VersionRange::and(a.clone(),b),VersionRange::and(a,c))))
    }

    #[test]
    fn or_distributes_over_and(a in range_gen(), b in range_gen(), c in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::or(a.clone(), VersionRange::and(b.clone(),c.clone()))) == obs.satisfies(&VersionRange::and(VersionRange::or(a.clone(),b),VersionRange::or(a,c))))
    }

    #[test]
    fn demorgans(a in range_gen(), b in range_gen(), obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::not(VersionRange::or(a.clone(), b.clone()))) == obs.satisfies(&VersionRange::and(VersionRange::not(a.clone()),VersionRange::not(b.clone()))));
        assert!(obs.satisfies(&VersionRange::not(VersionRange::and(a.clone(), b.clone()))) == obs.satisfies(&VersionRange::or(VersionRange::not(a),VersionRange::not(b))));
    }

    #[test]
    fn any_accepts_any(obs in ex_version_gen()) {
        assert!(obs.satisfies(&VersionRange::Any))
    }

    #[test]
    fn none_accepts_none(obs in ex_version_gen()) {
        assert!(!obs.satisfies(&VersionRange::None))
    }

    #[test]
    fn and_both(a in range_gen(), b in range_gen(), obs in ex_version_gen()) {
        assert!((obs.satisfies(&a) && obs.satisfies(&b)) == obs.satisfies(&VersionRange::and(a,b)))
    }

    #[test]
    fn or_either(a in range_gen(), b in range_gen(), obs in ex_version_gen()) {
        assert!((obs.satisfies(&a) || obs.satisfies(&b)) == obs.satisfies(&VersionRange::or(a,b)))
    }

    #[test]
    fn range_parse_round_trip(a in range_gen(), obs in ex_version_gen()) {
        match a.to_string().parse::<VersionRange>() {
            Ok(range) => {
                assert!(obs.satisfies(&a) == obs.satisfies(&range));
            }
            Err(e) => panic!("parse after display failed {}", e),
        }
    }

    #[test]
    fn version_equality_agrees_with_ordering((left, right) in version_pair_gen()) {
        // Ordered collections require `Eq` and `Ord` to agree.
        assert_eq!(left == right, left.cmp(&right) == Ordering::Equal);
    }

    #[test]
    fn numeric_prerelease_storage_agrees(value in any::<usize>()) {
        let number = PreReleaseSegment::Number(value);
        let big_number = PreReleaseSegment::BigNumber(value.to_string().into());
        assert_eq!(number, big_number);
        assert_eq!(big_number, number);
        assert_eq!(number.cmp(&big_number), Ordering::Equal);
        assert_eq!(big_number.cmp(&number), Ordering::Equal);
        assert_eq!(hash_of(&number), hash_of(&big_number));
    }

    #[test]
    fn release_of_one_agrees_with_satisfies(a in range_gen(), obs in ex_version_gen()) {
        assert!(a.satisfied_by_release(&[obs.clone()]) == obs.satisfies(&a))
    }

    #[test]
    fn release_reduce_preserves_satisfaction(a in range_gen(), x in ex_version_gen(), y in ex_version_gen()) {
        let versions = [x, y];
        assert!(a.clone().reduce().satisfied_by_release(&versions) == a.satisfied_by_release(&versions))
    }

    #[test]
    fn witness_implies_satisfiable(a in range_gen(), obs in ex_version_gen()) {
        // If a concrete version satisfies the range, the range is satisfiable.
        if obs.satisfies(&a) {
            assert!(a.satisfiable());
        }
    }

    #[test]
    fn intersects_symmetric(a in range_gen(), b in range_gen()) {
        assert!(a.intersects(&b) == b.intersects(&a));
    }
}

#[test]
fn satisfiable_basic() {
    // tautologies and contradictions
    assert!(VersionRange::Any.satisfiable());
    assert!(!VersionRange::None.satisfiable());

    // simple anchors are satisfiable
    let v: ExtendedVersion = "1.0:0".parse().unwrap();
    assert!(VersionRange::anchor(GTE, v.clone()).satisfiable());
    assert!(VersionRange::anchor(LT, v.clone()).satisfiable());
    assert!(VersionRange::anchor(EQ, v.clone()).satisfiable());
    assert!(VersionRange::anchor(NEQ, v.clone()).satisfiable());

    // disjoint comparison anchors of the same flavor: unsatisfiable
    let lo: ExtendedVersion = "2.0:0".parse().unwrap();
    let hi: ExtendedVersion = "1.0:0".parse().unwrap();
    let r = VersionRange::and(VersionRange::anchor(GTE, lo), VersionRange::anchor(LT, hi));
    assert!(!r.satisfiable());
}

#[test]
fn satisfiable_flavor_disjoint() {
    // a flavored range and a flavor-less range share no versions because
    // ExtendedVersion comparisons across flavors are incomparable.
    let knots: VersionRange = "^#knots:29:0".parse().unwrap();
    let core: VersionRange = "<=29.3:10".parse().unwrap();
    assert!(knots.satisfiable());
    assert!(core.satisfiable());
    assert!(!VersionRange::and(knots.clone(), core.clone()).satisfiable());
    assert!(!knots.intersects(&core));

    // two different concrete flavors also disjoint
    let a: ExtendedVersion = "#a:1:0".parse().unwrap();
    let b: ExtendedVersion = "#b:1:0".parse().unwrap();
    assert!(
        !VersionRange::and(VersionRange::anchor(EQ, a), VersionRange::anchor(EQ, b),).satisfiable()
    );

    // intersects: same flavor, overlapping ranges
    let r1: VersionRange = ">=#bitcoin:1:0".parse().unwrap();
    let r2: VersionRange = "<#bitcoin:5:0".parse().unwrap();
    assert!(r1.intersects(&r2));
}

#[test]
fn caret() {
    let thing = "^1.2.3.4"
        .parse::<VersionRange>()
        .map_err(|e| eprintln!("{e}"))
        .unwrap();
    println!("{}", thing);
    // match parse_atom(b"<0.0.0") {
    // Ok(a) => println!("{:#?}", a),
    // Err(e) => println!("{}", e),
    // }
    // use nom::bytes::complete::tag;
    // use nom::multi::separated_list;
    // println!("{:?}", parse_range(b"=0.0.0"));
    // println!("{:?}", decimal(b"1234"));
}

#[cfg(feature = "serde")]
#[test]
fn deser() {
    let _v: ExtendedVersion = serde_yaml::from_str("---\n0.2.5:0\n").unwrap();
}

fn release(versions: &[&str]) -> Vec<ExtendedVersion> {
    versions.iter().map(|v| v.parse().unwrap()).collect()
}

fn range(r: &str) -> VersionRange {
    r.parse().unwrap()
}

#[test]
fn unbounded_numeric_prerelease_vectors() {
    let vectors = [
        ("1-a:0", "1-0:0", Ordering::Greater),
        (
            "1-9007199254740991:0",
            "1-9007199254740992:0",
            Ordering::Less,
        ),
        (
            "1-9007199254740992:0",
            "1-9007199254740993:0",
            Ordering::Less,
        ),
        (
            "1-18446744073709551616000000000000000000:0",
            "1-18446744073709551616000000000000000001:0",
            Ordering::Less,
        ),
    ];

    for (left, right, ordering) in vectors {
        let left_text = left;
        let left: ExtendedVersion = left_text.parse().unwrap();
        let right: ExtendedVersion = right.parse().unwrap();
        assert_eq!(left.to_string(), left_text);
        assert_eq!(left.partial_cmp(&right), Some(ordering));
        assert_ne!(left, right);
    }

    let exact = "1-18446744073709551616000000000000000000:0";
    let version: ExtendedVersion = exact.parse().unwrap();
    assert_eq!(version.to_string(), exact);
    assert_eq!(version, exact.parse().unwrap());

    let bounded = range(">=1-9007199254740992:0 && <1-9007199254740993:0");
    assert!("1-9007199254740992:0"
        .parse::<ExtendedVersion>()
        .unwrap()
        .satisfies(&bounded));
    assert!(!"1-9007199254740993:0"
        .parse::<ExtendedVersion>()
        .unwrap()
        .satisfies(&bounded));

    for expression in [
        ">=1-9007199254740992:0 && <1-9007199254740993:0",
        "=1-18446744073709551616000000000000000000:0",
    ] {
        let range = range(expression);
        assert_eq!(
            range.satisfied_by_release(&[version.clone()]),
            version.satisfies(&range),
        );
        assert_eq!(
            range
                .clone()
                .reduce()
                .satisfied_by_release(&[version.clone()]),
            range.satisfied_by_release(&[version.clone()]),
        );
    }
}

#[test]
fn release_range_vectors() {
    let vectors: &[(&str, &[&str], bool)] = &[
        ("^2.62.2:1", &["#quantum:1.5.2:0", "2.63.23:0"], true),
        ("^0", &["0.9:0"], true),
        ("^0", &["1:0"], false),
        ("^0.0.0", &["0.9:0"], true),
        ("^0.0.0", &["1:0"], false),
        ("0", &["0.9:0"], true),
        ("0", &["1:0"], false),
        ("~0", &["0.0.9:0"], true),
        ("~0", &["0.1:0"], false),
        ("~0.0.0", &["0.0.9:0"], true),
        ("~0.0.0", &["0.1:0"], false),
        ("~1", &["1.0.9:0"], true),
        ("~1", &["1.1:0"], false),
        ("^0.0.3-beta", &["0.0.3:0"], true),
        ("^0.0.3-beta", &["0.0.4-alpha:0"], true),
        ("^0.0.3-beta", &["0.0.4-beta:0"], false),
        ("~1.2-beta", &["1.2:0"], true),
        ("~1.2-beta", &["1.3-alpha:0"], true),
        ("~1.2-beta", &["1.3-beta:0"], false),
        ("!^1:0", &["0.5:0", "3:0"], true),
        ("!~1.2:0", &["1.1:0", "1.3:0"], true),
        ("!(!^1:0)", &["0.5:0", "3:0"], false),
        (">=2:0 && <3:0", &["2.5:0", "4:0"], true),
        (">=2:0 && <3:0", &["1:0", "4:0"], false),
        (">=2:0 && <2:0", &["2:0", "1:0"], false),
        (">=2:0 || <2:0", &["2:0", "1:0"], true),
        ("<2:0 || >=3:0", &["2.5:0", "4:0"], true),
        (
            "(>=1:0 && <2:0) || (>=3:0 && <5:0)",
            &["1.5:0", "4:0"],
            true,
        ),
        ("!(>=2:0 && <3:0)", &["1:0", "4:0"], true),
        ("!(>=2:0 && <3:0)", &["2.5:0", "4:0"], false),
        ("!(>=2:0 && <3:0)", &["2.5:0"], false),
        ("!(>=2:0 && <3:0)", &[], false),
        ("!(>=2:0 && <2:0)", &["2:0", "1:0"], true),
        ("!(!>=2:0)", &["2:0", "1:0"], true),
        ("!(!>=2:0) && <2:0", &["2:0", "1:0"], false),
        ("!(!(>=2:0 && <3:0))", &["1:0", "4:0"], false),
        ("!(!(>=2:0 && <3:0))", &["2.5:0", "4:0"], true),
        ("!(>=2:0 || <2:0)", &["2:0", "1:0"], false),
        ("!(!(!>=2:0))", &["2:0", "1:0"], false),
        ("!(!=2.5:0)", &["2.5:0", "2.6:0"], true),
        ("!#knots && >=29.4:0", &["#knots:29.4:5", "29.4:5"], false),
        (">=2.0:0 && !=2.0:5", &["2.0:5", "2.0:4"], false),
        ("^28.4:21 && !=28.4:22", &["31.1:10", "28.4:21"], true),
        ("!=1.0:0", &["1.0.0:0"], false),
        ("#foo && >=2:0", &["#foo:1:0", "2:0"], false),
        ("(#foo && >=2:0) || =1:0", &["#foo:2:0", "1:0"], true),
        ("*", &[], false),
        ("!", &[], false),
        ("!*", &[], false),
    ];

    for (expression, versions, expected) in vectors {
        assert_eq!(
            range(expression).satisfied_by_release(&release(versions)),
            *expected,
            "{expression} against {versions:?}",
        );
    }
}

#[test]
fn raw_negations_preserve_complete_exclusions() {
    let interval = range(">=2:0 && <3:0");
    let exclusion = VersionRange::Not(Box::new(interval.clone()));
    let double_negation = VersionRange::Not(Box::new(exclusion.clone()));
    let inequality = VersionRange::Anchor(NEQ, "2.5:0".parse().unwrap());
    let negated_inequality = VersionRange::Not(Box::new(inequality));

    assert!(exclusion.satisfied_by_release(&release(&["1:0", "4:0"])));
    assert!(!exclusion.satisfied_by_release(&release(&["2.5:0", "4:0"])));
    assert!(!double_negation.satisfied_by_release(&release(&["1:0", "4:0"])));
    assert!(double_negation.satisfied_by_release(&release(&["2.5:0", "4:0"])));
    assert!(negated_inequality.satisfied_by_release(&release(&["2.5:0", "2.6:0"])));
    assert!(!negated_inequality.satisfied_by_release(&release(&["2.6:0"])));
}

#[test]
fn reduce_preserves_raw_release_rewrites() {
    let interval = range(">=2:0 && <3:0");
    let raw_ranges = [
        VersionRange::And(Box::new(VersionRange::Any), Box::new(interval.clone())),
        VersionRange::Or(Box::new(VersionRange::None), Box::new(interval.clone())),
        VersionRange::Not(Box::new(VersionRange::Not(Box::new(interval)))),
        VersionRange::Not(Box::new(VersionRange::Anchor(
            NEQ,
            "2.5:0".parse().unwrap(),
        ))),
    ];
    let releases = [
        release(&[]),
        release(&["1:0", "4:0"]),
        release(&["2.5:0", "4:0"]),
        release(&["2.5:0", "2.6:0"]),
    ];

    for raw in raw_ranges {
        for versions in &releases {
            assert_eq!(
                raw.satisfied_by_release(versions),
                raw.clone().reduce().satisfied_by_release(versions),
                "{raw} against {versions:?}",
            );
        }
    }
}

#[test]
fn range_sugar_is_expanded_before_release_evaluation() {
    let cases = [
        ("^0", ">=0:0 && <1:0"),
        ("^0.0.0", ">=0.0.0:0 && <1:0"),
        ("0", ">=0:0 && <1:0"),
        ("~0", ">=0:0 && <0.1:0"),
        ("~0.0.0", ">=0.0.0:0 && <0.1:0"),
        ("~1", ">=1:0 && <1.1:0"),
        ("^0.0.3-beta", ">=0.0.3-beta:0 && <0.0.4-beta:0"),
        ("~1.2-beta", ">=1.2-beta:0 && <1.3-beta:0"),
        ("1:0", ">=1:0 && <2:0"),
        ("^1:0", ">=1:0 && <2:0"),
        ("~1.2:0", ">=1.2:0 && <1.3:0"),
    ];

    for (expression, expanded) in cases {
        assert_eq!(range(expression), range(expanded), "{expression}");
    }

    for expression in ["^1:0", "~1.2:0", "!^1:0", "!~1.2:0"] {
        let range = range(expression);
        for version in release(&["0.5:0", "1.2:0", "1.3:0", "3:0"]) {
            assert_eq!(
                range.satisfied_by_release(&[version.clone()]),
                version.satisfies(&range),
                "{expression} against {version}",
            );
        }
    }
}
