import { Component, input } from '@angular/core'
import { IdentityKey, KeyIdentityPipe } from '@start9labs/marketplace'
import { TuiTitle } from '@taiga-ui/core'

@Component({
  selector: 'span[appKeyIdentity]',
  template: `
    @let identity = appKeyIdentity() | keyIdentity;
    <b>{{ identity.name }}</b>
    @for (email of identity.emails; track email) {
      <span tuiSubtitle>{{ email }}</span>
    }
    @for (fingerprint of identity.fingerprints; track fingerprint) {
      <code tuiSubtitle>{{ fingerprint }}</code>
    }
  `,
  styles: `
    code {
      overflow-wrap: anywhere;
      white-space: normal;
    }
  `,
  hostDirectives: [TuiTitle],
  imports: [KeyIdentityPipe],
})
export class KeyIdentity {
  public readonly appKeyIdentity = input.required<IdentityKey>()
}
