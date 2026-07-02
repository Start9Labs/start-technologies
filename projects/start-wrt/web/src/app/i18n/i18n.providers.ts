import { forwardRef, InjectionToken, signal } from '@angular/core'
import { tuiProvide } from '@taiga-ui/cdk'
import {
  TuiLanguageName,
  tuiLanguageSwitcher,
  TuiLanguageSwitcherService,
} from '@taiga-ui/i18n'
import { ENGLISH } from './dictionaries/en'
import { i18nService } from './i18n.service'

export type i18nKey = keyof typeof ENGLISH
export type i18n = Record<(typeof ENGLISH)[i18nKey], string>

/** Holds the active language's `id -> translation` dictionary (null = English). */
export const I18N = new InjectionToken('', {
  factory: () => signal<i18n | null>(null),
})

/** Lazy-loads a dictionary for the given Taiga language. */
export const I18N_LOADER = new InjectionToken<
  (lang: TuiLanguageName) => Promise<i18n>
>('')

export const I18N_PROVIDERS = [
  // Localizes Taiga's own built-in widget strings (date pickers, dialogs, etc.)
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
  {
    provide: I18N_LOADER,
    useValue: async (language: TuiLanguageName): Promise<unknown> => {
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
    },
  },
  tuiProvide(
    TuiLanguageSwitcherService,
    forwardRef(() => i18nService),
  ),
]
