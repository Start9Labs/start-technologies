import { Version, VersionRange, ExtendedVersion } from '../exver'
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
      testNormalization('!(=1 && =2)', '!(=1:0 && =2:0)')
      testNormalization('!=1 || !=2', '!=1:0 || !=2:0')
      testNormalization(
        '(!=#foo:1 || !=#foo:2) && #foo',
        '(!=#foo:1:0 || !=#foo:2:0) && #foo',
      )
      testNormalization('!=#foo:1 || !=#bar:2', '!=#foo:1:0 || !=#bar:2:0')
      testNormalization('!(=1 || =2)', '!(=1:0 || =2:0)')
      testNormalization('=1 && (=2 || =3)', '!')
      testNormalization('=1 && (=1 || =2)', '=1:0')
      testNormalization('=#foo:1 && =#bar:1', '!')
      testNormalization(
        '!(=#foo:1) && !(=#bar:1)',
        '!(=#foo:1:0) && !(=#bar:1:0)',
      )
      testNormalization(
        '!(=#foo:1) && !(=#bar:1) && >2',
        '!(=#foo:1:0) && !(=#bar:1:0) && >2:0',
      )
      testNormalization('~1.2.3', '>=1.2.3:0 && <1.3:0')
      testNormalization('^1.2.3', '>=1.2.3:0 && <2:0')
      testNormalization('^1.2.3 && >=1 && >=1.2 && >=1.3', '>=1.3:0 && <2:0')
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
    const numericVectors: [string, string, 'less' | 'greater'][] = [
      ['1-a:0', '1-0:0', 'greater'],
      ['1-9007199254740991:0', '1-9007199254740992:0', 'less'],
      ['1-9007199254740992:0', '1-9007199254740993:0', 'less'],
      [
        '1-18446744073709551616000000000000000000:0',
        '1-18446744073709551616000000000000000001:0',
        'less',
      ],
    ]

    test.each(numericVectors)(
      '%s compares %s than %s',
      (left, right, order) => {
        const parsedLeft = ExtendedVersion.parse(left)
        const parsedRight = ExtendedVersion.parse(right)
        expect(parsedLeft.toString()).toEqual(left)
        expect(parsedLeft.compare(parsedRight)).toEqual(order)
        expect(parsedLeft.equals(parsedRight)).toEqual(false)
      },
    )

    test('preserves the public representation for safe and unbounded identifiers', () => {
      const safe = ExtendedVersion.parse('1-9007199254740991:0')
      const unbounded = ExtendedVersion.parse(
        '1-18446744073709551616000000000000000000:0',
      )

      expect(safe.upstream.prerelease).toEqual([Number.MAX_SAFE_INTEGER])
      expect(unbounded.upstream.prerelease).toEqual([
        '18446744073709551616000000000000000000',
      ])
      expect(JSON.stringify(unbounded.upstream.prerelease)).toEqual(
        '["18446744073709551616000000000000000000"]',
      )
    })

    test('keeps exact range, release and normalization semantics', () => {
      const lower = ExtendedVersion.parse('1-9007199254740992:0')
      const upper = ExtendedVersion.parse('1-9007199254740993:0')
      const range = VersionRange.parse(
        '>=1-9007199254740992:0 && <1-9007199254740993:0',
      )
      const normalized = range.normalize()

      expect(lower.satisfies(range)).toEqual(true)
      expect(upper.satisfies(range)).toEqual(false)
      expect(range.satisfiedByRelease([lower])).toEqual(true)
      expect(range.satisfiedByRelease([upper])).toEqual(false)
      expect(normalized.satisfiedByRelease([lower])).toEqual(true)
      expect(normalized.satisfiedByRelease([upper])).toEqual(false)
      expect(normalized.toString()).toEqual(
        '>=1-9007199254740992:0 && <1-9007199254740993:0',
      )
    })

    test('keeps constructed digit strings in the string ordering domain', () => {
      expect(
        new ExtendedVersion(
          null,
          new Version([1], ['1']),
          new Version([0], []),
        ).compare(ExtendedVersion.parse('1-2:0')),
      ).toEqual('greater')
    })

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

  describe('release range evaluation', () => {
    const vectors: [string, string[], boolean][] = [
      ['^2.62.2:1', ['#quantum:1.5.2:0', '2.63.23:0'], true],
      ['^0', ['0.9:0'], true],
      ['^0', ['1:0'], false],
      ['^0.0.0', ['0.9:0'], true],
      ['^0.0.0', ['1:0'], false],
      ['0', ['0.9:0'], true],
      ['0', ['1:0'], false],
      ['~0', ['0.0.9:0'], true],
      ['~0', ['0.1:0'], false],
      ['~0.0.0', ['0.0.9:0'], true],
      ['~0.0.0', ['0.1:0'], false],
      ['~1', ['1.0.9:0'], true],
      ['~1', ['1.1:0'], false],
      ['^0.0.3-beta', ['0.0.3:0'], true],
      ['^0.0.3-beta', ['0.0.4-alpha:0'], true],
      ['^0.0.3-beta', ['0.0.4-beta:0'], false],
      ['~1.2-beta', ['1.2:0'], true],
      ['~1.2-beta', ['1.3-alpha:0'], true],
      ['~1.2-beta', ['1.3-beta:0'], false],
      ['!^1:0', ['0.5:0', '3:0'], true],
      ['!~1.2:0', ['1.1:0', '1.3:0'], true],
      ['!(!^1:0)', ['0.5:0', '3:0'], false],
      ['>=2:0 && <3:0', ['2.5:0', '4:0'], true],
      ['>=2:0 && <3:0', ['1:0', '4:0'], false],
      ['>=2:0 && <2:0', ['2:0', '1:0'], false],
      ['>=2:0 || <2:0', ['2:0', '1:0'], true],
      ['<2:0 || >=3:0', ['2.5:0', '4:0'], true],
      ['(>=1:0 && <2:0) || (>=3:0 && <5:0)', ['1.5:0', '4:0'], true],
      ['!(>=2:0 && <3:0)', ['1:0', '4:0'], true],
      ['!(>=2:0 && <3:0)', ['2.5:0', '4:0'], false],
      ['!(>=2:0 && <3:0)', ['2.5:0'], false],
      ['!(>=2:0 && <3:0)', [], false],
      ['!(>=2:0 && <2:0)', ['2:0', '1:0'], true],
      ['!(!>=2:0)', ['2:0', '1:0'], true],
      ['!(!>=2:0) && <2:0', ['2:0', '1:0'], false],
      ['!(!(>=2:0 && <3:0))', ['1:0', '4:0'], false],
      ['!(!(>=2:0 && <3:0))', ['2.5:0', '4:0'], true],
      ['!(>=2:0 || <2:0)', ['2:0', '1:0'], false],
      ['!(!(!>=2:0))', ['2:0', '1:0'], false],
      ['!(!=2.5:0)', ['2.5:0', '2.6:0'], true],
      ['!#knots && >=29.4:0', ['#knots:29.4:5', '29.4:5'], false],
      ['>=2.0:0 && !=2.0:5', ['2.0:5', '2.0:4'], false],
      ['^28.4:21 && !=28.4:22', ['31.1:10', '28.4:21'], true],
      ['!=1.0:0', ['1.0.0:0'], false],
      ['#foo && >=2:0', ['#foo:1:0', '2:0'], false],
      ['(#foo && >=2:0) || =1:0', ['#foo:2:0', '1:0'], true],
      ['*', [], false],
      ['!', [], false],
      ['!*', [], false],
    ]

    test.each(vectors)('%s against %j is %s', (range, versions, expected) => {
      expect(
        VersionRange.parse(range).satisfiedByRelease(
          versions.map(v => ExtendedVersion.parse(v)),
        ),
      ).toEqual(expected)
    })

    test.each([
      ['^0', '>=0:0 && <1:0'],
      ['^0.0.0', '>=0.0.0:0 && <1:0'],
      ['0', '>=0:0 && <1:0'],
      ['~0', '>=0:0 && <0.1:0'],
      ['~0.0.0', '>=0.0.0:0 && <0.1:0'],
      ['~1', '>=1:0 && <1.1:0'],
      ['^0.0.3-beta', '>=0.0.3-beta:0 && <0.0.4-beta:0'],
      ['~1.2-beta', '>=1.2-beta:0 && <1.3-beta:0'],
      ['1:0', '>=1:0 && <2:0'],
      ['^1:0', '>=1:0 && <2:0'],
      ['~1.2:0', '>=1.2:0 && <1.3:0'],
    ])('%s parses to %s', (expression, expanded) => {
      const range = VersionRange.parse(expression)
      expect(range).toEqual(VersionRange.parse(expanded))
      for (const version of ['0.5:0', '1.2:0', '1.3:0', '3:0'].map(v =>
        ExtendedVersion.parse(v),
      )) {
        expect(range.satisfiedByRelease([version])).toEqual(
          version.satisfies(range),
        )
      }
    })

    test('normalization preserves alias witnesses and release-wide vetoes', () => {
      const releases = [
        [],
        ['1:0'],
        ['2:0'],
        ['2:5', '2:6'],
        ['0.5:0', '3:0'],
        ['1.1:0', '1.3:0'],
        ['1:0', '4:0'],
        ['2.5:0', '4:0'],
        ['0.0.3:0', '0.0.4-alpha:0'],
        ['1.2:0', '1.3-alpha:0'],
        ['#foo:1:0', '2:0'],
        ['#knots:29.4:5', '29.4:5'],
      ].map(versions => versions.map(v => ExtendedVersion.parse(v)))
      const ranges = [
        '>=2:0 && <3:0',
        '^0',
        '0',
        '~0',
        '~1',
        '^0.0.3-beta',
        '~1.2-beta',
        '!^1:0',
        '!~1.2:0',
        '!(!^1:0)',
        '#foo && >=2:0',
        '(#foo && >=2:0) || =1:0',
        '!=2:5',
        '>=2:0 && !=2:5',
        '(>=2:0 && >=1:0) && !=2.5:0',
        '!(=2:5)',
        '!(>=2:0 && <3:0)',
        '!(!>=2:0)',
        '!(!>=2:0) && <2:0',
        '!(!(>=2:0 && <3:0))',
        '!(>=2:0 || <2:0)',
        '!(!(!>=2:0))',
        '!#knots && >=29.4:0',
        '(!=2:5 && >=2:0) || =1:0',
      ]

      const simplifiable = VersionRange.parse('(>=2:0 && >=1:0) && !=2.5:0')
      expect(simplifiable.normalize()).not.toEqual(simplifiable)

      for (const expression of ranges) {
        const range = VersionRange.parse(expression)
        const normalized = range.normalize()
        for (const release of releases) {
          expect(normalized.satisfiedByRelease(release)).toEqual(
            range.satisfiedByRelease(release),
          )
        }
      }
    })

    test('does not expand a release-wide inequality into per-alias comparisons', () => {
      const range = VersionRange.parse('!=2:5')
      const release = ['2:5', '2:6'].map(v => ExtendedVersion.parse(v))

      expect(range.normalize().satisfiedByRelease(release)).toEqual(false)
    })

    test('one declared version agrees with ordinary satisfaction', () => {
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
