import { inject, Injectable, signal } from '@angular/core'
import { TuiLanguageName, TuiLanguageSwitcherService } from '@taiga-ui/i18n'
import { I18N, I18N_LOADER, I18N_STORAGE } from './i18n.providers'
import { localizeFor } from './localize'
import { T } from '@start9labs/start-core'

export const languages = ['en_US', 'es_ES', 'de_DE', 'fr_FR', 'pl_PL'] as const
export type Languages = (typeof languages)[number]

/**
 * Maps POSIX locale strings to TUI language names
 */
export const LANGUAGE_TO_TUI: Record<Languages, TuiLanguageName> = {
  en_US: 'english',
  es_ES: 'spanish',
  de_DE: 'german',
  fr_FR: 'french',
  pl_PL: 'polish',
}

@Injectable({
  providedIn: 'root',
})
export class i18nService extends TuiLanguageSwitcherService {
  private readonly i18n = inject(I18N)
  private readonly i18nLoader = inject(I18N_LOADER)
  private readonly store = inject(I18N_STORAGE)

  readonly loading = signal(false)

  /**
   * Current language as POSIX locale string
   */
  get lang(): Languages {
    return (
      (Object.entries(LANGUAGE_TO_TUI).find(
        ([, tui]) => tui === this.language,
      )?.[0] as Languages) || 'en_US'
    )
  }

  localize(string: T.LocaleString, defaultLocale?: string | null): string {
    return localizeFor(string, this.lang, defaultLocale)
  }

  setLangLocal(language: Languages = 'en_US'): void {
    const tuiLang = LANGUAGE_TO_TUI[language]
    super.setLanguage(tuiLang)
    this.loading.set(true)
    this.i18nLoader(tuiLang).then(value => {
      this.i18n.set(value)
      this.loading.set(false)
    })
  }

  setLang(language: Languages = 'en_US'): void {
    const tuiLang = LANGUAGE_TO_TUI[language]
    if (this.language === tuiLang) {
      this.setLangLocal(language)
    } else {
      this.store(language).then(() => this.setLangLocal(language))
    }
  }
}
