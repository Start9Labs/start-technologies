import { inject, Injectable, Pipe, PipeTransform } from '@angular/core'
import { I18N } from '@start9labs/shared'
import { ENGLISH } from './dictionaries/en'
import { i18nKey } from './i18n.providers'

/**
 * Translates an English source string to the active language, falling back to
 * the English key itself when no translation exists. Mirrors the shared
 * i18nPipe but resolves ids against StartTunnel's own ENGLISH map (the shared
 * `I18N` signal holds the active `id -> string` dictionary).
 */
@Pipe({
  name: 'i18n',
  pure: false,
})
@Injectable({ providedIn: 'root' })
export class i18nPipe implements PipeTransform {
  private readonly i18n = inject(I18N)

  transform(englishKey: i18nKey | null | undefined | ''): string {
    englishKey = englishKey || ('' as i18nKey)

    const id = ENGLISH[englishKey]

    return (id !== undefined && this.i18n()?.[id]) || englishKey
  }
}
