import { setupI18n } from '../i18n'

const dict = { 'keep ${n} of them': 0, plain: 1 } as const
const translations = { es_ES: { 0: 'quedan ${n}', 1: 'simple' } }

function withLang<A>(lang: string | undefined, fn: () => A): A {
  const previous = process.env.LANG
  if (lang === undefined) delete process.env.LANG
  else process.env.LANG = lang
  try {
    return fn()
  } finally {
    if (previous === undefined) delete process.env.LANG
    else process.env.LANG = previous
  }
}

describe('setupI18n', () => {
  // A StartOS service container runs with LANG=C.UTF-8, which reduces to the
  // POSIX locale `C`. Intl rejects it, so before this was resolved once at
  // setup, every interpolated number or Date threw RangeError on the server
  // while working on the developer's machine.
  test.each(['C.UTF-8', 'C', 'POSIX'])(
    'interpolates a number under LANG=%s',
    lang => {
      withLang(lang, () => {
        const i18n = setupI18n(dict, translations, 'en_US')
        expect(i18n('keep ${n} of them', { n: 20 })).toBe('keep 20 of them')
      })
    },
  )

  test('interpolates a Date under a POSIX locale', () => {
    withLang('C.UTF-8', () => {
      const i18n = setupI18n(dict, translations, 'en_US')
      expect(() => i18n('keep ${n} of them', { n: new Date(0) })).not.toThrow()
    })
  })

  test('still formats numbers per locale where Intl accepts one', () => {
    withLang('de_DE.UTF-8', () => {
      const i18n = setupI18n(
        dict,
        { de_DE: { 0: 'noch ${n}', 1: 'einfach' } },
        'en_US',
      )
      // de-DE groups with a period; the point is that the locale still applies.
      expect(i18n('keep ${n} of them', { n: 1234567 })).toBe('noch 1.234.567')
    })
  })

  test('translates and passes strings through untouched', () => {
    withLang('es_ES.UTF-8', () => {
      const i18n = setupI18n(dict, translations, 'en_US')
      expect(i18n('plain')).toBe('simple')
      expect(i18n('keep ${n} of them', { n: '20' })).toBe('quedan 20')
    })
  })

  test('falls back to the default language when LANG is unset', () => {
    withLang(undefined, () => {
      const i18n = setupI18n(dict, translations, 'en_US')
      expect(i18n('plain')).toBe('plain')
      expect(i18n('keep ${n} of them', { n: 7 })).toBe('keep 7 of them')
    })
  })
})
