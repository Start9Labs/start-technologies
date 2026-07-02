import {
  computed,
  effect,
  inject,
  Injectable,
  InjectionToken,
  signal,
} from '@angular/core'
import { I18N } from 'src/app/i18n/i18n.providers'
import { i18nService } from 'src/app/i18n/i18n.service'
import { Language } from 'src/app/utils/languages'
import HELP_EN from './content/en'

export const HELP_OPEN = new InjectionToken('Help sidebar open status', {
  factory: () => signal(false),
})

export const HELP_URL = new InjectionToken<string>('Help URL')

/**
 * Lazy loaders for non-English help content. English is bundled eagerly (it is
 * the source of truth + fallback); the other languages are fetched only when
 * selected, mirroring the i18n dictionary loaders in `i18n.providers.ts`.
 */
const HELP_LOADERS: Record<
  Exclude<Language, 'en_US'>,
  () => Promise<Record<string, string>>
> = {
  es_ES: () => import('./content/es').then(m => m.default),
  de_DE: () => import('./content/de').then(m => m.default),
  fr_FR: () => import('./content/fr').then(m => m.default),
  pl_PL: () => import('./content/pl').then(m => m.default),
}

@Injectable({ providedIn: 'root' })
export class HelpService {
  private readonly i18nService = inject(i18nService)
  private readonly i18n = inject(I18N)
  private readonly loaded = signal<
    Partial<Record<Language, Record<string, string>>>
  >({ en_US: HELP_EN })

  /**
   * Route -> help content resolved to the active language, falling back to
   * English. Recomputes on language switch (and once a lazily-loaded language
   * finishes loading).
   */
  readonly content = computed<Record<string, string>>(() => {
    this.i18n() // re-run on language switch (the dictionary signal flips)
    const lang = this.i18nService.lang
    const translation = this.loaded()[lang]
    const out: Record<string, string> = {}
    for (const route of Object.keys(HELP_EN)) {
      out[route] = (lang !== 'en_US' && translation?.[route]) || HELP_EN[route]
    }
    return out
  })

  constructor() {
    // Fetch the active language's help content on switch (English is eager).
    effect(() => {
      this.i18n()
      const lang = this.i18nService.lang
      if (lang !== 'en_US' && !this.loaded()[lang]) {
        HELP_LOADERS[lang]().then(data =>
          this.loaded.update(m => ({ ...m, [lang]: data })),
        )
      }
    })
  }
}

export function provideHelp(useValue: string) {
  return { provide: HELP_URL, useValue }
}
