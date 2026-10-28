import { forwardRef, InjectionToken, signal } from '@angular/core'
import { tuiProvide } from '@taiga-ui/cdk'
import {
  TuiLanguageName,
  tuiLanguageSwitcher,
  TuiLanguageSwitcherService,
} from '@taiga-ui/i18n'
import { ENGLISH } from './dictionaries/en'
import { i18nService, Languages } from './i18n.service'

export type i18nKey = keyof typeof ENGLISH
export type i18n = Record<(typeof ENGLISH)[i18nKey], string>

export const I18N = new InjectionToken('', {
  factory: () => signal<i18n | null>(null),
})

export const I18N_LOADER = new InjectionToken<
  (lang: TuiLanguageName) => Promise<i18n | null>
>('')

export const I18N_STORAGE = new InjectionToken<
  (lang: Languages) => Promise<void>
>('', {
  factory: () => () => Promise.resolve(),
})

export async function loadDictionary(
  language: TuiLanguageName,
): Promise<i18n | null> {
  switch (language) {
    case 'spanish':
      return import('./dictionaries/es').then(v => v.default)
    case 'polish':
      return import('./dictionaries/pl').then(v => v.default)
    case 'german':
      return import('./dictionaries/de').then(v => v.default)
    case 'french':
      return import('./dictionaries/fr').then(v => v.default)
    default:
      return null
  }
}

export const I18N_PROVIDERS = [
  tuiLanguageSwitcher(async (language: TuiLanguageName): Promise<unknown> => {
    switch (language) {
      case 'spanish':
        return import('@taiga-ui/i18n/languages/spanish')
      case 'polish':
        return import('@taiga-ui/i18n/languages/polish')
      case 'german':
        return import('@taiga-ui/i18n/languages/german')
      case 'french':
        return import('@taiga-ui/i18n/languages/french')
      default:
        return import('@taiga-ui/i18n/languages/english')
    }
  }),
  { provide: I18N_LOADER, useValue: loadDictionary },
  tuiProvide(
    TuiLanguageSwitcherService,
    forwardRef(() => i18nService),
  ),
]
