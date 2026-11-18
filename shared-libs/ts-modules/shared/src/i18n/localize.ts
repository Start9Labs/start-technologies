import type { T } from '@start9labs/start-core'

/**
 * The requested locale's exact or same-language translation, else the owner's
 * declared default, else an arbitrary stored one.
 * Matches the backend's `LocaleString::localize_for`.
 */
export function localizeFor(
  string: T.LocaleString,
  lang: string,
  defaultLocale?: string | null,
): string {
  if (typeof string === 'string') return string

  const language = lang.split('_')[0] + '_'
  return (
    string[lang] ??
    Object.entries(string).find(([locale]) =>
      locale.startsWith(language),
    )?.[1] ??
    (defaultLocale ? string[defaultLocale] : undefined) ??
    Object.values(string)[0] ??
    ''
  )
}
