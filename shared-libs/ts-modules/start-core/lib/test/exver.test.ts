import { VersionRange, ExtendedVersion } from '../exver'
describe('ExVer', () => {
  {
    {
      const checker = VersionRange.parse('*')
      test("VersionRange.parse('*')", () => {
        checker.satisfiedBy(ExtendedVersion.parse('1:0'))
        checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))
        checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))
        checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))
        checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.5'))
        checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.5.6'))
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          true,
        )
      })
      test("VersionRange.parse('*') invalid", () => {
        expect(() => checker.satisfiedBy(ExtendedVersion.parse('a'))).toThrow()
        expect(() => checker.satisfiedBy(ExtendedVersion.parse(''))).toThrow()
        expect(() =>
          checker.satisfiedBy(ExtendedVersion.parse('1..3')),
        ).toThrow()
      })
    }

    {
      const checker = VersionRange.parse('>1.2.3:4')
      test(`VersionRange.parse(">1.2.3:4") valid`, () => {
        expect(
          checker.satisfiedBy(ExtendedVersion.parse('2-beta.123:0')),
        ).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:5'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.1'))).toEqual(
          true,
        )
      })

      test(`VersionRange.parse(">1.2.3:4") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
      })
    }
    {
      const checker = VersionRange.parse('=1.2.3')
      test(`VersionRange.parse("=1.2.3") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          true,
        )
      })

      test(`VersionRange.parse("=1.2.3") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:1'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          false,
        )
      })
    }
    {
      // TODO: this this correct? if not, also fix normalize
      const checker = VersionRange.parse('=1')
      test(`VersionRange.parse("=1") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.0.0:0'))).toEqual(
          true,
        )
      })

      test(`VersionRange.parse("=1") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.0.1:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.0.0:1'))).toEqual(
          false,
        )
      })
    }
    {
      const checker = VersionRange.parse('>=1.2.3:4')
      test(`VersionRange.parse(">=1.2.3:4") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:5'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.1'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          true,
        )
      })

      test(`VersionRange.parse(">=1.2.3:4") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
      })
    }
    {
      const checker = VersionRange.parse('<1.2.3:4')
      test(`VersionRange.parse("<1.2.3:4") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:5'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.1'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          false,
        )
      })

      test(`VersionRange.parse("<1.2.3:4") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)
      })
    }
    {
      const checker = VersionRange.parse('<=1.2.3:4')
      test(`VersionRange.parse("<=1.2.3:4") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:5'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.1'))).toEqual(
          false,
        )
      })

      test(`VersionRange.parse("<=1.2.3:4") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          true,
        )
      })
    }

    {
      const checkA = VersionRange.parse('>1')
      const checkB = VersionRange.parse('<=2')

      const checker = checkA.and(checkB)
      test(`simple and(checkers) valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          true,
        )
      })
      test(`simple and(checkers) invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2.1:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
      })
    }
    {
      const checkA = VersionRange.parse('<1')
      const checkB = VersionRange.parse('=2')

      const checker = checkA.or(checkB)
      test(`simple or(checkers) valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('0.1:0'))).toEqual(
          true,
        )
      })
      test(`simple or(checkers) invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2.1:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          false,
        )
      })
    }

    {
      const checker = VersionRange.parse('~1.2')
      test(`VersionRange.parse(~1.2) valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.1:0'))).toEqual(
          true,
        )
      })
      test(`VersionRange.parse(~1.2) invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3.1:0'))).toEqual(
          false,
        )

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1.1:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)

        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(false)
      })
    }

    {
      const checker = VersionRange.parse('~1.2').not()
      test(`VersionRange.parse(~1.2).not() valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3.1:0'))).toEqual(
          true,
        )

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1.1:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)

        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
      })
      test(`VersionRange.parse(~1.2).not() invalid `, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.1:0'))).toEqual(
          false,
        )
      })
    }
    {
      const checker = VersionRange.parse('!~1.2')
      test(`!(VersionRange.parse(~1.2)) valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.3.1:0'))).toEqual(
          true,
        )

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1.1:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)

        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
      })
      test(`!(VersionRange.parse(~1.2)) invalid `, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.1:0'))).toEqual(
          false,
        )
      })
    }
    {
      const checker = VersionRange.parse('!>1.2.3:4')
      test(`VersionRange.parse("!>1.2.3:4") invalid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:5'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4.1'))).toEqual(
          false,
        )
      })

      test(`VersionRange.parse("!>1.2.3:4") valid`, () => {
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:4'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)
      })
    }

    {
      function testNormalization(input: string, expected: string) {
        test(`"${input}" normalizes to "${expected}"`, () => {
          const checker = VersionRange.parse(input).normalize()
          expect(checker.toString()).toEqual(expected)
        })
      }

      testNormalization('=2.0', '=2.0:0')
      testNormalization('=1 && =2', '!')
      testNormalization('!(=1 && =2)', '*')
      testNormalization('!=1 || !=2', '*')
      testNormalization('(!=#foo:1 || !=#foo:2) && #foo', '#foo')
      testNormalization(
        '!=#foo:1 || !=#bar:2',
        '<#foo:1:0 || >#foo:1:0 || !#foo || <#bar:2:0 || >#bar:2:0 || !#bar',
      )
      testNormalization('!(=1 || =2)', '<1:0 || (>1:0 && <2:0) || >2:0 || !#')
      testNormalization('=1 && (=2 || =3)', '!')
      testNormalization('=1 && (=1 || =2)', '=1:0')
      testNormalization('=#foo:1 && =#bar:1', '!')
      testNormalization(
        '!(=#foo:1) && !(=#bar:1)',
        '<#foo:1:0 || >#foo:1:0 || <#bar:1:0 || >#bar:1:0 || (!#foo && !#bar)',
      )
      testNormalization('!(=#foo:1) && !(=#bar:1) && >2', '>2:0')
      testNormalization('~1.2.3', '>=1.2.3:0 && <1.3.0:0')
      testNormalization('^1.2.3', '>=1.2.3:0 && <2.0.0:0')
      testNormalization(
        '^1.2.3 && >=1 && >=1.2 && >=1.3',
        '>=1.3:0 && <2.0.0:0',
      )
      testNormalization(
        '(>=1.0 && <1.1) || (>=1.1 && <1.2) || (>=1.2 && <1.3)',
        '>=1.0:0 && <1.3:0',
      )
      testNormalization('>1 || <2', '#')

      testNormalization('=1 && =1.2 && =1.2.3', '!')
      // testNormalization("=1 && =1.2 && =1.2.3", "=1.2.3:0"); TODO: should it be this instead?
      testNormalization('=1 || =1.2 || =1.2.3', '=1:0 || =1.2:0 || =1.2.3:0')
      // testNormalization("=1 || =1.2 || =1.2.3", "=1:0"); TODO: should it be this instead?
    }

    {
      test('>1 && =1.2', () => {
        const checker = VersionRange.parse('>1 && =1.2')

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.1:0'))).toEqual(
          false,
        )
      })
      test('=1 || =2', () => {
        const checker = VersionRange.parse('=1 || =2')

        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          false,
        ) // really?
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2.3:0'))).toEqual(
          false,
        ) // really?
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('3:0'))).toEqual(false)
      })

      test('>1 && =1.2 || =2', () => {
        const checker = VersionRange.parse('>1 && =1.2 || =2')

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.2:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)
        expect(checker.satisfiedBy(ExtendedVersion.parse('3:0'))).toEqual(false)
      })

      test('&& before || order of operationns:  <1.5 && >1 || >1.5 && <3', () => {
        const checker = VersionRange.parse('<1.5 && >1 || >1.5 && <3')
        expect(checker.satisfiedBy(ExtendedVersion.parse('1.1:0'))).toEqual(
          true,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('2:0'))).toEqual(true)

        expect(checker.satisfiedBy(ExtendedVersion.parse('1.5:0'))).toEqual(
          false,
        )
        expect(checker.satisfiedBy(ExtendedVersion.parse('1:0'))).toEqual(false)
        expect(checker.satisfiedBy(ExtendedVersion.parse('3:0'))).toEqual(false)
      })

      test('Compare function on the emver', () => {
        const a = ExtendedVersion.parse('1.2.3:0')
        const b = ExtendedVersion.parse('1.2.4:0')

        expect(a.compare(b)).toEqual('less')
        expect(b.compare(a)).toEqual('greater')
        expect(a.compare(a)).toEqual('equal')
      })
      test('Compare for sort function on the emver', () => {
        const a = ExtendedVersion.parse('1.2.3:0')
        const b = ExtendedVersion.parse('1.2.4:0')

        expect(a.compareForSort(b)).toEqual(-1)
        expect(b.compareForSort(a)).toEqual(1)
        expect(a.compareForSort(a)).toEqual(0)
      })
    }
  }

  // `tables()` distinguishes points by (upstream, downstream, side). Ranges that
  // differ only in the downstream revision are the case that regressed: they
  // collapsed into one another, so `normalize()` silently dropped the lower.
  describe('downstream revisions are distinct points', () => {
    const eq = (v: string) => VersionRange.anchor('=', ExtendedVersion.parse(v))

    test('normalize preserves a union of same-upstream downstream revisions', () => {
      const union = eq('1.0.0:0').or(eq('1.0.0:1')).normalize()

      expect(union.satisfiedBy(ExtendedVersion.parse('1.0.0:0'))).toEqual(true)
      expect(union.satisfiedBy(ExtendedVersion.parse('1.0.0:1'))).toEqual(true)
      expect(union.satisfiedBy(ExtendedVersion.parse('1.0.0:2'))).toEqual(false)
    })

    test('normalize preserves the whole span below a downstream revision', () => {
      // The shape a VersionGraph produces for `other: [1.0.0:3]`, current 1.0.0:15.
      const reachable = VersionRange.none()
        .or(eq('1.0.0:15'))
        .or(
          VersionRange.anchor('>=', ExtendedVersion.parse('1.0.0:3')).and(
            VersionRange.anchor('<', ExtendedVersion.parse('1.0.0:15')),
          ),
        )
        .or(eq('1.0.0:3'))
        .or(VersionRange.anchor('<', ExtendedVersion.parse('1.0.0:3')))
        .normalize()

      for (const v of ['1.0.0:0', '1.0.0:3', '1.0.0:4', '1.0.0:14', '1.0.0:15'])
        expect(reachable.satisfiedBy(ExtendedVersion.parse(v))).toEqual(true)
    })

    test('intersects distinguishes downstream revisions', () => {
      expect(eq('1.0.0:0').intersects(eq('1.0.0:1'))).toEqual(false)
      expect(eq('1.0.0:1').intersects(eq('1.0.0:1'))).toEqual(true)
    })
  })

  describe('lexicographic ordering across flavors', () => {
    const unflavored = ExtendedVersion.parse('1.0.0:0')
    const flavored = ExtendedVersion.parse('#quantum:1.0.0:0')

    test('orders a flavor against no flavor in both directions', () => {
      expect(unflavored.compareLexicographic(flavored)).toEqual('less')
      expect(flavored.compareLexicographic(unflavored)).toEqual('greater')
    })

    test('compareForSort stays inside its return type', () => {
      expect(unflavored.compareForSort(flavored)).toEqual(-1)
      expect(flavored.compareForSort(unflavored)).toEqual(1)
    })

    test('sorts a mixed-flavor list the same whatever order it arrives in', () => {
      const fromFlavored = [flavored, unflavored].sort((a, b) =>
        a.compareForSort(b),
      )
      const fromUnflavored = [unflavored, flavored].sort((a, b) =>
        a.compareForSort(b),
      )
      expect(fromFlavored.map(v => v.toString())).toEqual([
        '1.0.0:0',
        '#quantum:1.0.0:0',
      ])
      expect(fromUnflavored.map(v => v.toString())).toEqual(
        fromFlavored.map(v => v.toString()),
      )
    })
  })

  describe('prerelease segments', () => {
    test('accepts a segment mixing letters, digits and hyphens', () => {
      for (const v of [
        '1.0.0-rc1:0',
        '1.0.0-beta2:0',
        '1.0.0-alpha-1:0',
        '1.0.0-1a:0',
        '1.0.0-x-y-z:0',
      ]) {
        expect(ExtendedVersion.parse(v).toString()).toEqual(v)
      }
    })

    test('rejects a numeric segment with a leading zero', () => {
      expect(() => ExtendedVersion.parse('1.0.0-01:0')).toThrow()
    })

    test('rejects an empty segment', () => {
      expect(() => ExtendedVersion.parse('1.0.0-a..b:0')).toThrow()
    })

    test('orders a numeric segment below a string segment at any position', () => {
      expect(
        ExtendedVersion.parse('1.0.0-a.b.1:0').compare(
          ExtendedVersion.parse('1.0.0-a.b.c:0'),
        ),
      ).toEqual('less')
      expect(
        ExtendedVersion.parse('1.0.0-a.b.c:0').compare(
          ExtendedVersion.parse('1.0.0-a.b.1:0'),
        ),
      ).toEqual('greater')
    })
  })

  describe('a release satisfies a range through the versions it declares', () => {
    const release = (installed: string, satisfies: string[]) =>
      [installed, ...satisfies].map(v => ExtendedVersion.parse(v))

    test('an alias carries a positive range', () => {
      expect(
        VersionRange.parse('^2.62.2:1').satisfiedByRelease(
          release('#quantum:1.5.2:0', ['2.63.23:0']),
        ),
      ).toEqual(true)
    })

    test('an alias does not escape an excluded revision', () => {
      expect(
        VersionRange.parse('>=2.0:0 && !=2.0:5').satisfiedByRelease(
          release('2.0:5', ['2.0:4']),
        ),
      ).toEqual(false)
    })

    test('an alias does not escape a negated flavor', () => {
      expect(
        VersionRange.parse('!#knots && >=29.4:0').satisfiedByRelease(
          release('#knots:29.4:5', ['29.4:5']),
        ),
      ).toEqual(false)
    })

    test('an exclusion matching nothing the release carries still passes', () => {
      expect(
        VersionRange.parse('^28.4:21 && !=28.4:22').satisfiedByRelease(
          release('31.1:10', ['28.4:21']),
        ),
      ).toEqual(true)
    })

    test('a release of one version answers as that version does', () => {
      for (const range of [
        '>=2.0:0',
        '!=2.0:5',
        '^2.0:0',
        '!(>=2.0:0)',
        '#knots',
        '!#knots',
        '*',
        '>=2.0:0 && !=2.0:5',
      ]) {
        for (const v of ['2.0:5', '2.0:4', '#knots:29.4:5']) {
          const parsed = ExtendedVersion.parse(v)
          expect(
            VersionRange.parse(range).satisfiedByRelease([parsed]),
          ).toEqual(parsed.satisfies(VersionRange.parse(range)))
        }
      }
    })
  })
})
