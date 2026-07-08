import { forwardRef } from '@angular/core'
import {
  I18N_LOADER,
  I18N_STORAGE,
  i18nService,
  Languages,
} from '@start9labs/shared'
import { tuiProvide } from '@taiga-ui/cdk'
import {
  TuiLanguageName,
  tuiLanguageSwitcher,
  TuiLanguageSwitcherService,
} from '@taiga-ui/i18n'
import { ENGLISH } from './dictionaries/en'

/** Every English source string passed to the i18n pipe is a key of ENGLISH. */
export type i18nKey = keyof typeof ENGLISH

/** localStorage key holding the user's chosen POSIX locale. */
export const LANG_STORAGE_KEY = 'start-tunnel:lang'

/**
 * StartTunnel i18n wiring. Reuses the shared `i18nService`, the `I18N` /
 * `I18N_LOADER` / `I18N_STORAGE` tokens, and Taiga's language switcher — only
 * the loader (pointed at StartTunnel's own dictionaries) and the storage
 * strategy are local. StartTunnel has no server-side language field, so the
 * choice is persisted in localStorage. Until a dictionary under `./dictionaries`
 * is populated, its load resolves to an empty map and the pipe falls back to the
 * English key.
 */
export const TUNNEL_I18N_PROVIDERS = [
  // Localize Taiga's own built-in widget strings (dialogs, date pickers, etc.).
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
  {
    provide: I18N_STORAGE,
    useValue: (language: Languages): Promise<void> => {
      localStorage.setItem(LANG_STORAGE_KEY, language)
      return Promise.resolve()
    },
  },
  tuiProvide(
    TuiLanguageSwitcherService,
    forwardRef(() => i18nService),
  ),
]
