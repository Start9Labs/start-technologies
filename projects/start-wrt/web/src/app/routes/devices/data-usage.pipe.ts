import { inject, Pipe, PipeTransform } from '@angular/core'
import { i18nService } from 'src/app/i18n/i18n.service'
import { formatDataUsageMb } from './data-usage'

@Pipe({
  name: 'dataUsageMb',
  pure: false,
})
export class DataUsageMbPipe implements PipeTransform {
  private readonly i18n = inject(i18nService)

  transform(bytes: number | null | undefined): string | null {
    return formatDataUsageMb(bytes, this.i18n.locale())
  }
}
