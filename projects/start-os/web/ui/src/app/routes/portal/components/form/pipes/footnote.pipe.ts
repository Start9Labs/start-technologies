import { Pipe, PipeTransform } from '@angular/core'
import { escapeHtml } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'

@Pipe({
  name: 'footnote',
})
export class FootnotePipe implements PipeTransform {
  transform(spec: Exclude<IST.ValueSpec, IST.ValueSpecHidden>): string | null {
    return 'footnote' in spec && spec.footnote
      ? escapeHtml(spec.footnote)
      : null
  }
}
