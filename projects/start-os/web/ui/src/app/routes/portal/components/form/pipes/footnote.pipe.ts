import { Pipe, PipeTransform } from '@angular/core'
import { IST } from '@start9labs/start-core'

import { injectInlineMarkdown } from './inline-markdown'

@Pipe({
  name: 'footnote',
})
export class FootnotePipe implements PipeTransform {
  private readonly markdown = injectInlineMarkdown()

  transform(spec: Exclude<IST.ValueSpec, IST.ValueSpecHidden>): string | null {
    return 'footnote' in spec && spec.footnote
      ? this.markdown(spec.footnote)
      : null
  }
}
