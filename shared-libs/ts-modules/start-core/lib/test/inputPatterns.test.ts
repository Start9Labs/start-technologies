import { InputSpec } from '../actions/input/builder/inputSpec'
import { List } from '../actions/input/builder/list'
import { Value } from '../actions/input/builder/value'

const goDuration = {
  regex: '^([0-9]+(s|m|h))+$',
  description: 'Must be a number followed by s, m, or h',
}

/** Unanchored on purpose — the form anchors it, and so must the parser. */
const lowercase = {
  regex: '[a-z]+',
  description: 'May only contain lower case letters',
}

async function parserFor(value: Value<any, any>) {
  return (await value.build({} as any)).validator
}

describe('action input patterns', () => {
  test('a conforming value parses', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Timeout',
        required: true,
        default: null,
        patterns: [goDuration],
      }),
    )
    expect(parser.parse('2h')).toBe('2h')
  })

  test('a non-conforming value is rejected with the pattern description', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Timeout',
        required: true,
        default: null,
        patterns: [goDuration],
      }),
    )
    expect(() => parser.parse('2min')).toThrow(goDuration.description)
  })

  test('an unanchored pattern is anchored, as Validators.pattern does', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Name',
        required: true,
        default: null,
        patterns: [lowercase],
      }),
    )
    expect(parser.parse('abc')).toBe('abc')
    expect(() => parser.parse('abc1')).toThrow(lowercase.description)
  })

  test('an already-anchored pattern is not double-anchored', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Timeout',
        required: true,
        default: null,
        patterns: [goDuration],
      }),
    )
    expect(parser.parse('1h30m')).toBe('1h30m')
  })

  test('every pattern must hold', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Name',
        required: true,
        default: null,
        patterns: [lowercase, { regex: '.{3,}', description: 'Too short' }],
      }),
    )
    expect(parser.parse('abc')).toBe('abc')
    expect(() => parser.parse('ab')).toThrow('Too short')
  })

  test('an empty value defers to required rather than failing the pattern', async () => {
    const parser = await parserFor(
      Value.text({
        name: 'Timeout',
        required: false,
        default: null,
        patterns: [goDuration],
      }),
    )
    expect(parser.parse('')).toBe('')
    expect(parser.parse(null)).toBe(null)
  })

  test('a field with no patterns is unconstrained', async () => {
    const parser = await parserFor(
      Value.text({ name: 'Any', required: true, default: null }),
    )
    expect(parser.parse('anything at all')).toBe('anything at all')
  })

  test('dynamic text resolves its patterns at build time', async () => {
    const parser = await parserFor(
      Value.dynamicText(async () => ({
        name: 'Timeout',
        required: true as const,
        default: null,
        patterns: [goDuration],
      })),
    )
    expect(parser.parse('45m')).toBe('45m')
    expect(() => parser.parse('45min')).toThrow(goDuration.description)
  })

  test('list items are held to their patterns', async () => {
    const parser = await parserFor(
      Value.list(List.text({ name: 'Timeouts' }, { patterns: [goDuration] })),
    )
    expect(parser.parse(['2h', '45m'])).toEqual(['2h', '45m'])
    expect(() => parser.parse(['2h', '45min'])).toThrow(goDuration.description)
  })

  test('the whole input spec parses through its fields', async () => {
    const built = await InputSpec.of({
      timeout: Value.text({
        name: 'Timeout',
        required: false,
        default: null,
        patterns: [goDuration],
      }),
    }).build({} as any)

    expect(built.validator.parse({ timeout: '2h' })).toEqual({ timeout: '2h' })
    expect(() => built.validator.parse({ timeout: '2min' })).toThrow(
      goDuration.description,
    )
  })
})
