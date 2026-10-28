import { InputSpec } from '@start9labs/start-core/actions/input/builder/inputSpec'
import { List } from '@start9labs/start-core/actions/input/builder/list'
import { Value } from '@start9labs/start-core/actions/input/builder/value'
import { Variants } from '@start9labs/start-core/actions/input/builder/variants'
import { ValueSpec } from '@start9labs/start-core/actions/input/inputSpecTypes'
import { setupManifest } from '../manifest/setupManifest'
import { StartSdk } from '../StartSdk'

export type IfEquals<T, U, Y = unknown, N = never> =
  (<G>() => G extends T ? 1 : 2) extends <G>() => G extends U ? 1 : 2 ? Y : N
export function testOutput<A, B>(): (c: IfEquals<A, B>) => null {
  return () => null
}

describe('builder tests', () => {
  test('text', async () => {
    const bitcoinPropertiesBuilt: {
      'peer-tor-address': ValueSpec
    } = await InputSpec.of({
      'peer-tor-address': Value.text({
        name: 'Peer tor address',
        description: 'The Tor address of the peer interface',
        required: true,
        default: null,
      }),
    })
      .build({} as any)
      .then(a => a.spec)
    expect(bitcoinPropertiesBuilt).toMatchObject({
      'peer-tor-address': {
        type: 'text',
        description: 'The Tor address of the peer interface',
        warning: null,
        masked: false,
        placeholder: null,
        minLength: null,
        maxLength: null,
        patterns: [],
        disabled: false,
        inputmode: 'text',
        name: 'Peer tor address',
        required: true,
        default: null,
      },
    })
  })
})

describe('values', () => {
  test('toggle', async () => {
    const value = await Value.toggle({
      name: 'Testing',
      description: null,
      warning: null,
      default: false,
    }).build({} as any)
    const validator = value.validator
    validator.parse(false)
    testOutput<typeof validator._output, boolean>()(null)
  })
  test('triState', async () => {
    const value = await Value.triState({
      name: 'Testing',
      default: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse(true)
    validator.parse(false)
    validator.parse(null)
    expect(() => validator.parse('on')).toThrow()
    testOutput<typeof validator._output, boolean | null>()(null)
    expect(value.spec).toMatchObject({
      type: 'triState',
      name: 'Testing',
      default: null,
      footnote: null,
    })
  })
  test('text', async () => {
    const value = await Value.text({
      name: 'Testing',
      required: true,
      default: null,
    }).build({} as any)
    const validator = value.validator
    const rawIs = value.spec
    validator.parse('test text')
    expect(() => validator.parse(null)).toThrow()
    testOutput<typeof validator._output, string>()(null)
  })
  test('text with default', async () => {
    const value = await Value.text({
      name: 'Testing',
      required: true,
      default: 'this is a default value',
    }).build({} as any)
    const validator = value.validator
    const rawIs = value.spec
    validator.parse('test text')
    expect(() => validator.parse(null)).toThrow()
    testOutput<typeof validator._output, string>()(null)
  })
  test('text footnote propagates to spec', async () => {
    const value = await Value.text({
      name: 'Testing',
      required: false,
      default: null,
      footnote: 'Defaults to 1024 upstream',
    }).build({} as any)
    expect(value.spec).toMatchObject({
      type: 'text',
      footnote: 'Defaults to 1024 upstream',
    })
  })
  test('text footnote defaults to null', async () => {
    const value = await Value.text({
      name: 'Testing',
      required: false,
      default: null,
    }).build({} as any)
    expect(value.spec).toMatchObject({ type: 'text', footnote: null })
  })
  test('optional text', async () => {
    const value = await Value.text({
      name: 'Testing',
      required: false,
      default: null,
    }).build({} as any)
    const validator = value.validator
    const rawIs = value.spec
    validator.parse('test text')
    validator.parse(null)
    testOutput<typeof validator._output, string | null>()(null)
  })
  test('color', async () => {
    const value = await Value.color({
      name: 'Testing',
      required: false,
      default: null,
      description: null,
      warning: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('#000000')
    testOutput<typeof validator._output, string | null>()(null)
  })
  test('datetime', async () => {
    const value = await Value.datetime({
      name: 'Testing',
      required: true,
      default: null,
      description: null,
      warning: null,
      inputmode: 'date',
      min: null,
      max: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('2021-01-01')
    testOutput<typeof validator._output, string>()(null)
  })
  test('optional datetime', async () => {
    const value = await Value.datetime({
      name: 'Testing',
      required: false,
      default: null,
      description: null,
      warning: null,
      inputmode: 'date',
      min: null,
      max: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('2021-01-01')
    testOutput<typeof validator._output, string | null>()(null)
  })
  test('textarea', async () => {
    const value = await Value.textarea({
      name: 'Testing',
      required: false,
      default: null,
      description: null,
      warning: null,
      minLength: null,
      maxLength: null,
      minRows: 3,
      maxRows: 6,
      placeholder: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('test text')
    testOutput<typeof validator._output, string | null>()(null)
  })
  test('number', async () => {
    const value = await Value.number({
      name: 'Testing',
      required: true,
      default: null,
      integer: false,
      description: null,
      warning: null,
      min: null,
      max: null,
      step: null,
      units: null,
      placeholder: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse(2)
    testOutput<typeof validator._output, number>()(null)
  })
  test('optional number', async () => {
    const value = await Value.number({
      name: 'Testing',
      required: false,
      default: null,
      integer: false,
      description: null,
      warning: null,
      min: null,
      max: null,
      step: null,
      units: null,
      placeholder: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse(2)
    testOutput<typeof validator._output, number | null>()(null)
  })
  test('select', async () => {
    const value = await Value.select({
      name: 'Testing',
      default: 'a',
      values: {
        a: 'A',
        b: 'B',
      },
      description: null,
      warning: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('a')
    validator.parse('b')
    expect(() => validator.parse('c')).toThrow()
    testOutput<typeof validator._output, 'a' | 'b'>()(null)
  })
  test('nullable select', async () => {
    const value = await Value.select({
      name: 'Testing',
      default: 'a',
      values: {
        a: 'A',
        b: 'B',
      },
      description: null,
      warning: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse('a')
    validator.parse('b')
    testOutput<typeof validator._output, 'a' | 'b'>()(null)
  })
  test('multiselect', async () => {
    const value = await Value.multiselect({
      name: 'Testing',
      values: {
        a: 'A',
        b: 'B',
      },
      default: [],
      description: null,
      warning: null,
      minLength: null,
      maxLength: null,
    }).build({} as any)
    const validator = value.validator
    validator.parse([])
    validator.parse(['a', 'b'])

    expect(() => validator.parse(['e'])).toThrow()
    expect(() => validator.parse([4])).toThrow()
    testOutput<typeof validator._output, Array<'a' | 'b'>>()(null)
  })
  test('object', async () => {
    const value = await Value.object(
      {
        name: 'Testing',
        description: null,
      },
      InputSpec.of({
        a: Value.toggle({
          name: 'test',
          description: null,
          warning: null,
          default: false,
        }),
      }),
    ).build({} as any)
    const validator = value.validator
    validator.parse({ a: true })
    testOutput<typeof validator._output, { a: boolean }>()(null)
  })
  test('union', async () => {
    const value = await Value.union({
      name: 'Testing',
      default: 'a',
      description: null,
      warning: null,
      variants: Variants.of({
        a: {
          name: 'a',
          spec: InputSpec.of({
            b: Value.toggle({
              name: 'b',
              description: null,
              warning: null,
              default: false,
            }),
          }),
        },
      }),
    }).build({} as any)
    const validator = value.validator
    validator.parse({ selection: 'a', value: { b: false } })
    type Test = typeof validator._output
    testOutput<
      Test,
      {
        selection: 'a'
        value: {
          b: boolean
        }
        other?: {}
      }
    >()(null)
  })

  describe('dynamic', () => {
    const fakeOptions = {
      inputSpec: 'inputSpec',
      effects: 'effects',
      utils: 'utils',
    } as any
    test('toggle', async () => {
      const value = await Value.dynamicToggle(async () => ({
        name: 'Testing',
        description: null,
        warning: null,
        default: false,
      })).build({} as any)
      const validator = value.validator
      validator.parse(false)
      expect(() => validator.parse(null)).toThrow()
      testOutput<typeof validator._output, boolean>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        description: null,
        warning: null,
        default: false,
      })
    })
    test('triState', async () => {
      const value = await Value.dynamicTriState(async () => ({
        name: 'Testing',
        default: null,
        footnote: 'Leave unset to use upstream default',
      })).build({} as any)
      const validator = value.validator
      validator.parse(true)
      validator.parse(false)
      validator.parse(null)
      testOutput<typeof validator._output, boolean | null>()(null)
      expect(value.spec).toMatchObject({
        type: 'triState',
        name: 'Testing',
        default: null,
        footnote: 'Leave unset to use upstream default',
      })
    })
    test('text', async () => {
      const value = await Value.dynamicText(async () => ({
        name: 'Testing',
        required: false,
        default: null,
      })).build({} as any)
      const validator = value.validator
      const rawIs = value.spec
      validator.parse('test text')
      validator.parse(null)
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
        default: null,
      })
    })
    test('text with default', async () => {
      const value = await Value.dynamicText(async () => ({
        name: 'Testing',
        required: false,
        default: 'this is a default value',
      })).build({} as any)
      const validator = value.validator
      validator.parse('test text')
      validator.parse(null)
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
        default: 'this is a default value',
      })
    })
    test('optional text', async () => {
      const value = await Value.dynamicText(async () => ({
        name: 'Testing',
        required: false,
        default: null,
      })).build({} as any)
      const validator = value.validator
      const rawIs = value.spec
      validator.parse('test text')
      validator.parse(null)
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
        default: null,
      })
    })
    test('color', async () => {
      const value = await Value.dynamicColor(async () => ({
        name: 'Testing',
        required: false,
        default: null,
        description: null,
        warning: null,
      })).build({} as any)
      const validator = value.validator
      validator.parse('#000000')
      validator.parse(null)
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
        default: null,
        description: null,
        warning: null,
      })
    })
    test('datetime', async () => {
      const sdk = StartSdk.of()
        .withManifest(
          setupManifest({
            id: 'testOutput',
            title: '',
            license: '',
            packageRepo: '',
            upstreamRepo: '',
            marketingUrl: '',
            donationUrl: null,
            description: {
              short: '',
              long: '',
            },
            images: {},
            volumes: [],
          }),
        )
        .build(true)

      const value = await Value.dynamicDatetime(async ({ effects }) => {
        return {
          name: 'Testing',
          required: false,
          default: null,
          inputmode: 'date',
        }
      }).build({} as any)
      const validator = value.validator
      validator.parse('2021-01-01')
      validator.parse(null)
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
        default: null,
        description: null,
        warning: null,
        inputmode: 'date',
      })
    })
    test('textarea', async () => {
      const value = await Value.dynamicTextarea(async () => ({
        name: 'Testing',
        required: false,
        default: null,
        description: null,
        warning: null,
        minLength: null,
        maxLength: null,
        minRows: 3,
        maxRows: 6,
        placeholder: null,
      })).build({} as any)
      const validator = value.validator
      validator.parse('test text')
      testOutput<typeof validator._output, string | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
      })
    })
    test('number', async () => {
      const value = await Value.dynamicNumber(() => ({
        name: 'Testing',
        required: false,
        default: null,
        integer: false,
        description: null,
        warning: null,
        min: null,
        max: null,
        step: null,
        units: null,
        placeholder: null,
      })).build({} as any)
      const validator = value.validator
      validator.parse(2)
      validator.parse(null)
      expect(() => validator.parse('null')).toThrow()
      testOutput<typeof validator._output, number | null>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        required: false,
      })
    })
    test('select', async () => {
      const value = await Value.dynamicSelect(() => ({
        name: 'Testing',
        default: 'a',
        values: {
          a: 'A',
          b: 'B',
        },
        description: null,
        warning: null,
      })).build({} as any)
      const validator = value.validator
      validator.parse('a')
      validator.parse('b')
      testOutput<typeof validator._output, 'a' | 'b'>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
      })
    })
    test('multiselect', async () => {
      const value = await Value.dynamicMultiselect(() => ({
        name: 'Testing',
        values: {
          a: 'A',
          b: 'B',
        },
        default: [],
        description: null,
        warning: null,
        minLength: null,
        maxLength: null,
      })).build({} as any)
      const validator = value.validator
      validator.parse([])
      validator.parse(['a', 'b'])

      expect(() => validator.parse([4])).toThrow()
      expect(() => validator.parse(null)).toThrow()
      testOutput<typeof validator._output, Array<'a' | 'b'>>()(null)
      expect(value.spec).toMatchObject({
        name: 'Testing',
        default: [],
      })
    })
  })
  describe('filtering', () => {
    test('union', async () => {
      const value = await Value.dynamicUnion(() => ({
        name: 'Testing',
        default: 'a',
        description: null,
        warning: null,
        disabled: ['a', 'c'],
        variants: Variants.of({
          a: {
            name: 'a',
            spec: InputSpec.of({
              b: Value.toggle({
                name: 'b',
                description: null,
                warning: null,
                default: false,
              }),
            }),
          },
          b: {
            name: 'b',
            spec: InputSpec.of({
              b: Value.toggle({
                name: 'b',
                description: null,
                warning: null,
                default: false,
              }),
            }),
          },
        }),
      })).build({} as any)
      const validator = value.validator
      validator.parse({ selection: 'a', value: { b: false } })
      type Test = typeof validator._output
      testOutput<
        Test,
        | {
            selection: 'a'
            value: {
              b: boolean
            }
            other?: {
              b?: {
                b?: boolean
              }
            }
          }
        | {
            selection: 'b'
            value: {
              b: boolean
            }
            other?: {
              a?: {
                b?: boolean
              }
            }
          }
      >()(null)

      const built = value.spec
      expect(built).toMatchObject({
        name: 'Testing',
        variants: {
          b: {},
        },
      })
      expect(built).toMatchObject({
        name: 'Testing',
        variants: {
          a: {},
          b: {},
        },
      })
      expect(built).toMatchObject({
        name: 'Testing',
        variants: {
          a: {},
          b: {},
        },
        disabled: ['a', 'c'],
      })
    })
  })
  test('dynamic union', async () => {
    const value = await Value.dynamicUnion(() => ({
      disabled: ['a', 'c'],
      name: 'Testing',
      default: 'b',
      description: null,
      warning: null,
      variants: Variants.of({
        a: {
          name: 'a',
          spec: InputSpec.of({
            b: Value.toggle({
              name: 'b',
              description: null,
              warning: null,
              default: false,
            }),
          }),
        },
        b: {
          name: 'b',
          spec: InputSpec.of({
            b: Value.toggle({
              name: 'b',
              description: null,
              warning: null,
              default: false,
            }),
          }),
        },
      }),
    })).build({} as any)
    const validator = value.validator
    validator.parse({ selection: 'a', value: { b: false } })
    type Test = typeof validator._output
    testOutput<
      Test,
      | {
          selection: 'a'
          value: {
            b: boolean
          }
          other?: {
            b?: {
              b?: boolean
            }
          }
        }
      | {
          selection: 'b'
          value: {
            b: boolean
          }
          other?: {
            a?: {
              b?: boolean
            }
          }
        }
    >()(null)

    const built = value.spec
    expect(built).toMatchObject({
      name: 'Testing',
      variants: {
        b: {},
      },
    })
    expect(built).toMatchObject({
      name: 'Testing',
      variants: {
        a: {},
        b: {},
      },
    })
    expect(built).toMatchObject({
      name: 'Testing',
      variants: {
        a: {},
        b: {},
      },
      disabled: ['a', 'c'],
    })
  })
})

describe('Builder List', () => {
  test('obj', async () => {
    const value = await Value.list(
      List.obj(
        {
          name: 'test',
        },
        {
          spec: InputSpec.of({
            test: Value.toggle({
              name: 'test',
              description: null,
              warning: null,
              default: false,
            }),
          }),
        },
      ),
    ).build({} as any)
    const validator = value.validator
    validator.parse([{ test: true }])
    testOutput<typeof validator._output, { test: boolean }[]>()(null)
  })
  test('text', async () => {
    const value = await Value.list(
      List.text(
        {
          name: 'test',
        },
        {
          patterns: [],
        },
      ),
    ).build({} as any)
    const validator = value.validator
    validator.parse(['test', 'text'])
    testOutput<typeof validator._output, string[]>()(null)
  })
  describe('dynamic', () => {
    test('text', async () => {
      const value = await Value.list(
        List.dynamicText(() => ({
          name: 'test',
          spec: { patterns: [] },
        })),
      ).build({} as any)
      const validator = value.validator
      validator.parse(['test', 'text'])
      expect(() => validator.parse([3, 4])).toThrow()
      expect(() => validator.parse(null)).toThrow()
      testOutput<typeof validator._output, string[]>()(null)
      expect(value.spec).toMatchObject({
        name: 'test',
        spec: { patterns: [] },
      })
    })
  })
})

describe('Nested nullable values', () => {
  test('Testing text', async () => {
    const value = await InputSpec.of({
      a: Value.text({
        name: 'Temp Name',
        description:
          'If no name is provided, the name from inputSpec will be used',
        required: false,
        default: null,
      }),
    }).build({} as any)
    const validator = value.validator
    validator.parse({ a: null })
    validator.parse({ a: 'test' })
    expect(() => validator.parse({ a: 4 })).toThrow()
    testOutput<typeof validator._output, { a: string | null }>()(null)
  })
  test('Testing number', async () => {
    const value = await InputSpec.of({
      a: Value.number({
        name: 'Temp Name',
        description:
          'If no name is provided, the name from inputSpec will be used',
        required: false,
        default: null,
        warning: null,
        placeholder: null,
        integer: false,
        min: null,
        max: null,
        step: null,
        units: null,
      }),
    }).build({} as any)
    const validator = value.validator
    validator.parse({ a: null })
    validator.parse({ a: 5 })
    expect(() => validator.parse({ a: '4' })).toThrow()
    testOutput<typeof validator._output, { a: number | null }>()(null)
  })
  test('Testing color', async () => {
    const value = await InputSpec.of({
      a: Value.color({
        name: 'Temp Name',
        description:
          'If no name is provided, the name from inputSpec will be used',
        required: false,
        default: null,
        warning: null,
      }),
    }).build({} as any)
    const validator = value.validator
    validator.parse({ a: null })
    validator.parse({ a: '5' })
    expect(() => validator.parse({ a: 4 })).toThrow()
    testOutput<typeof validator._output, { a: string | null }>()(null)
  })
  test('Testing select', async () => {
    const value = await InputSpec.of({
      a: Value.select({
        name: 'Temp Name',
        description:
          'If no name is provided, the name from inputSpec will be used',
        default: 'a',
        warning: null,
        values: {
          a: 'A',
        },
      }),
    }).build({} as any)
    const higher = await Value.select({
      name: 'Temp Name',
      description:
        'If no name is provided, the name from inputSpec will be used',
      default: 'a',
      warning: null,
      values: {
        a: 'A',
      },
    }).build({} as any)

    const validator = value.validator
    validator.parse({ a: 'a' })
    expect(() => validator.parse({ a: '4' })).toThrow()
    testOutput<typeof validator._output, { a: 'a' }>()(null)
  })
  test('Testing multiselect', async () => {
    const value = await InputSpec.of({
      a: Value.multiselect({
        name: 'Temp Name',
        description:
          'If no name is provided, the name from inputSpec will be used',

        warning: null,
        default: [],
        values: {
          a: 'A',
        },
        minLength: null,
        maxLength: null,
      }),
    }).build({} as any)
    const validator = value.validator
    validator.parse({ a: [] })
    validator.parse({ a: ['a'] })
    expect(() => validator.parse({ a: ['4'] })).toThrow()
    expect(() => validator.parse({ a: '4' })).toThrow()
    testOutput<typeof validator._output, { a: 'a'[] }>()(null)
  })
})
