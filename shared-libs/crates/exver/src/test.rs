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
            PreReleaseSegment::Number(string.parse().unwrap())
        } else {
            PreReleaseSegment::String(string.into())
        }
    }
}

prop_compose! {
    fn version_gen()(
        number in prop::collection::vec(any::<usize>(), 1..10),
        prerelease in prop::collection::vec(prop_oneof![
            any::<usize>().prop_map(PreReleaseSegment::Number),
            prerelease_string(),
        ], 0..3),
    ) -> Version {
        Version::new(number, prerelease)
    }
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
