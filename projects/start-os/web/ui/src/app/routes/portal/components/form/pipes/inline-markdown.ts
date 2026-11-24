import { inject, SecurityContext } from '@angular/core'
import { NgDompurifySanitizer } from '@taiga-ui/dompurify'
import { marked } from 'marked'

export function injectInlineMarkdown(): (text: string) => string {
  const sanitizer = inject(NgDompurifySanitizer)

  return text =>
    sanitizer.sanitize(SecurityContext.HTML, marked.parseInline(text))
}
