export const languages = ['en_US', 'es_ES', 'de_DE', 'fr_FR', 'pl_PL'] as const
export type Language = (typeof languages)[number]

/**
 * Language definition with metadata
 */
export interface LanguageInfo {
  posix: Language
  /** Endonym — the language's name in itself; shown as-is, never translated. */
  nativeName: string
  /** English name; translated through the i18n dictionary (`{{ name | i18n }}`). */
  name: string
}

/**
 * Available languages with their metadata
 */
export const LANGUAGES: LanguageInfo[] = [
  { posix: 'en_US', nativeName: 'English', name: 'English' },
  { posix: 'es_ES', nativeName: 'Español', name: 'Spanish' },
  { posix: 'de_DE', nativeName: 'Deutsch', name: 'German' },
  { posix: 'fr_FR', nativeName: 'Français', name: 'French' },
  { posix: 'pl_PL', nativeName: 'Polski', name: 'Polish' },
]
