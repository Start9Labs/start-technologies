import { inject, Injectable, Pipe, PipeTransform } from '@angular/core'
import { I18N, I18N_IDS, i18nKey } from './i18n.providers'

@Pipe({
  name: 'i18n',
  pure: false,
})
@Injectable({ providedIn: 'root' })
export class i18nPipe implements PipeTransform {
  private readonly i18n = inject(I18N)
  private readonly ids = inject(I18N_IDS)

  transform(englishKey: i18nKey | null | undefined | ''): string {
    englishKey = englishKey || ('' as i18nKey)

    const id = this.ids[englishKey]

    return (id !== undefined && this.i18n()?.[id]) || englishKey
  }
}
