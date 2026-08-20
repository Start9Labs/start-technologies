/**
 * Internationalization (i18n) utilities for StartOS packages.
 *
 * @example
 * ```typescript
 * // In package's i18n/index.ts:
 * import { setupI18n } from '@start9labs/start-sdk'
 * import defaultDict, { DEFAULT_LANG } from './dictionaries/default'
 * import translations from './dictionaries/translations'
 *
 * export const i18n = setupI18n(defaultDict, translations, DEFAULT_LANG)
 * ```
 */

type ParamValue = string | number | Date

/**
 * The first candidate `Intl` will construct with, or `undefined` for the
 * runtime default — which is always valid. POSIX locale names (`C`, `POSIX`)
 * are rejected by `Intl` even though they are perfectly valid `LANG` values.
 */
function firstUsableLocale(...candidates: string[]): string | undefined {
  for (const candidate of candidates) {
    try {
      new Intl.NumberFormat(candidate)
      return candidate
    } catch {}
  }
  return undefined
}

/**
 * Creates a typed i18n function for a package.
 *
 * @param defaultDict - The default language dictionary mapping strings to numeric indices
 * @param translations - Translation dictionaries for each supported locale
 * @param defaultLang - The default language code (e.g., 'en_US')
 * @returns A typed i18n function that accepts dictionary keys and optional parameters
 */
export function setupI18n<
  Dict extends Record<string, number>,
  Translations extends Record<string, Record<number, string>>,
>(defaultDict: Dict, translations: Translations, defaultLang: string) {
  const lang = process.env.LANG?.replace(/\.UTF-8$/, '') || defaultLang

  // Convert locale format from en_US to en-US for Intl APIs, and settle on one
  // Intl will actually accept. A service container runs with LANG=C.UTF-8, and
  // `new Intl.NumberFormat('C')` throws RangeError — so without this every
  // number or Date interpolated into a translated string throws at runtime, on
  // the server only, having worked on the developer's machine. Resolved once
  // here rather than guarded at each call site.
  const intlLocale = firstUsableLocale(
    lang.replace('_', '-'),
    defaultLang.replace('_', '-'),
  )

  function getTranslation(): Record<number, string> | null {
    if (lang === defaultLang) return null

    const availableLangs = Object.keys(translations) as (keyof Translations)[]

    const match =
      availableLangs.find(l => l === lang) ??
      availableLangs.find(l => String(l).startsWith(lang.split('_')[0] + '_'))

    return match ? (translations[match] as Record<number, string>) : null
  }

  const translation = getTranslation()

  function formatValue(value: ParamValue): string {
    if (typeof value === 'number') {
      return new Intl.NumberFormat(intlLocale).format(value)
    }
    if (value instanceof Date) {
      return new Intl.DateTimeFormat(intlLocale).format(value)
    }
    return value
  }

  return function i18n(
    key: keyof Dict,
    params?: Record<string, ParamValue>,
  ): string {
    let result = translation
      ? translation[defaultDict[key as string]]
      : (key as string)

    if (params) {
      for (const [paramName, value] of Object.entries(params)) {
        result = result.replace(`\${${paramName}}`, formatValue(value))
      }
    }

    return result
  }
}
