import { Component, computed, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'

@Component({
  selector: 'port-check-warnings',
  template: `
    @if (nothingListening()) {
      <p class="g-warning">
        {{
          'Nothing responded on this port, so its status cannot be determined'
            | i18n
        }}
      </p>
    }
    @if (hairpinning()) {
      <p class="g-warning">
        {{
          'This address will not work from your local network due to a router hairpinning limitation'
            | i18n
        }}
      </p>
    }
  `,
  styles: `
    p {
      margin-top: 0.5rem;
    }
  `,
  imports: [i18nPipe],
})
export class PortCheckWarningsComponent {
  // Either a full IPv4 check or the IPv6 sub-result.
  readonly result = input<T.CheckPortRes | T.CheckPortV6Res>()
  // Whether this port receives local traffic. Gates the hairpinning warning.
  readonly local = input(true)

  // Explains the icon's warning triangle, so it must match its condition: a
  // port that answered externally is reachable regardless of what the internal
  // probe sampled.
  readonly nothingListening = computed(
    (res = this.result()) =>
      !!res && !res.openExternally && !res.openInternally,
  )

  // Hairpinning is an IPv4 NAT artifact; the IPv6 sub-result has no such field.
  readonly hairpinning = computed(
    (res = this.result()) =>
      this.local() &&
      !!res &&
      'hairpinning' in res &&
      res.openExternally &&
      !res.hairpinning,
  )
}
