import type { T } from '@start9labs/start-core'

/**
 * The requested locale's exact or same-language translation, else the owner's
 * declared default, else the alphabetically first locale's.
 * Ties resolve alphabetically, never by key order, matching the backend's
 * `LocaleString::localize_for`.
 */
export function localizeFor(
  string: T.LocaleString,
  lang: string,
  defaultLocale?: string | null,
): string {
  if (typeof string === 'string') return string

  const language = lang.split('_')[0] + '_'
  const locales = Object.keys(string).sort()
  const sameLanguage = locales.find(locale => locale.startsWith(language))
  return (
    string[lang] ??
    (sameLanguage && string[sameLanguage]) ??
    (defaultLocale ? string[defaultLocale] : undefined) ??
    (locales[0] && string[locales[0]]) ??
    ''
  )
}
