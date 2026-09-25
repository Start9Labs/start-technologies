import {
  computed,
  Directive,
  inject,
  InjectionToken,
  input,
} from '@angular/core'
import { tuiSetSignal } from '@taiga-ui/cdk'
import { TuiHintDirective } from '@taiga-ui/core'
import { i18nPipe } from '../i18n/i18n.pipe'

export const VERSION = new InjectionToken<string>('VERSION', {
  factory: () => '',
})

@Directive({
  selector: '[docsLink]',
  hostDirectives: [TuiHintDirective],
  host: {
    target: '_blank',
    rel: 'noreferrer',
    '[attr.href]': 'url()',
  },
})
export class DocsLinkDirective {
  private readonly version = inject(VERSION)

  readonly path = input.required<string>()
  readonly fragment = input<string>('')

  protected readonly url = computed(() => {
    const path = this.path()
    const relative = path.startsWith('/') ? path : `/${path}`

    const query = this.version ? `?version=${this.version}` : ''

    return `https://docs.start9.com${relative}${query}${this.fragment()}`
  })

  constructor() {
    tuiSetSignal(
      inject(TuiHintDirective).content,
      inject(i18nPipe).transform('Documentation'),
    )
  }
}
