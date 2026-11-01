import { Component, input, Pipe } from '@angular/core'
import { StoreIconDirective } from '@start9labs/marketplace'
import { T } from '@start9labs/start-core'
import { TuiCell, TuiTitle, tuiCellOptionsProvider } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'

@Pipe({ name: 'registryTitle' })
class RegistryTitlePipe {
  transform(url: string, identity: T.RegistryIdentity | null) {
    return identity?.name || new URL(url).hostname
  }
}

@Component({
  selector: 'div[appRegistryOrigin]',
  template: `
    <span tuiAvatar size="xs">
      <img
        [storeIcon]="appRegistryOrigin()"
        [storeIconData]="identity()?.icon ?? null"
      />
    </span>
    <span tuiTitle>{{ appRegistryOrigin() | registryTitle: identity() }}</span>
  `,
  hostDirectives: [TuiCell],
  providers: [tuiCellOptionsProvider({ size: 's' })],
  imports: [RegistryTitlePipe, StoreIconDirective, TuiAvatar, TuiTitle],
})
export class RegistryOrigin {
  public readonly appRegistryOrigin = input.required<string>()
  public readonly identity = input<T.RegistryIdentity | null>(null)
}
