import { inject, Injectable } from '@angular/core'
import { TuiLanguageName, TuiLanguageSwitcherService } from '@taiga-ui/i18n'
import { Language } from 'src/app/utils/languages'
import { I18N, I18N_LOADER } from './i18n.providers'

/**
 * Maps POSIX locale strings to TUI language names.
 */
export const LANGUAGE_TO_TUI: Record<Language, TuiLanguageName> = {
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

  /**
   * Current language as a POSIX locale string.
   */
  get lang(): Language {
    return (
      (Object.entries(LANGUAGE_TO_TUI).find(
        ([, tui]) => tui === this.language,
      )?.[0] as Language) || 'en_US'
    )
  }

  /** Apply a language locally (no persistence). */
  setLangLocal(language: Language = 'en_US'): void {
    const tuiLang = LANGUAGE_TO_TUI[language]
    super.setLanguage(tuiLang)
    this.i18nLoader(tuiLang).then(value => {
      this.i18n.set(value)
    })
  }
}
