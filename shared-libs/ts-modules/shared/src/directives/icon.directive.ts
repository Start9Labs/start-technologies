import {
  computed,
  Directive,
  linkedSignal,
  Signal,
  signal,
} from '@angular/core'

export const FALLBACK_ICON = 'assets/img/service-icons/fallback.png'

/** An async icon `<img>` with fallback and image-loading state. */
@Directive({
  host: {
    alt: '',
    '[attr.src]': 'src() && failed() === src() ? fallback : src()',
    '(load)': 'settled.set(true)',
    '(error)': 'failed.set(src()); settled.set(true)',
  },
})
export abstract class IconDirective {
  protected abstract readonly src: Signal<string | null>
  protected readonly fallback = FALLBACK_ICON
  protected readonly failed = signal<string | null>(null)
  protected readonly settled = linkedSignal({
    source: () => this.src(),
    computation: () => false,
  })

  readonly loading = computed(() => !this.src() || !this.settled())
}
