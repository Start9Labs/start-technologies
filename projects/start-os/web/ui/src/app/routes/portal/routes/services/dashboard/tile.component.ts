import { Component, computed, input } from '@angular/core'
import { TuiCell, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { TuiCardLarge, tuiCardOptionsProvider } from '@taiga-ui/layout'
import { ServiceUptimeComponent } from 'src/app/routes/portal/routes/services/components/uptime.component'
import { PkgDependencyErrors } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { StatusComponent } from './status.component'

@Component({
  selector: 'a[appServiceTile]',
  template: `
    <span tuiCell>
      <span tuiAvatar [round]="false">
        <img alt="" [src]="pkg().icon" />
      </span>
      <span tuiTitle>
        <b>{{ manifest().title }}</b>
        <span tuiSubtitle>{{ manifest().version }}</span>
      </span>
    </span>
    <span class="status">
      <app-status [pkg]="pkg()" [hasDepErrors]="hasError()" />
      @if (pkg().statusInfo.started; as started) {
        <service-uptime [started]="started" />
      }
    </span>
  `,
  styles: `
    :host {
      // "floating" paints elevation-1, which is the g-page ground itself.
      --tui-background-elevation-1: color-mix(
        in hsl,
        var(--start9-base-2) 75%,
        transparent
      );

      color: var(--tui-text-primary);
    }

    .status {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      color: var(--tui-text-secondary);
    }
  `,
  host: { class: 'service-tile' },
  hostDirectives: [TuiCardLarge],
  providers: [
    tuiCardOptionsProvider({ space: 'compact', appearance: 'floating' }),
  ],
  imports: [
    ServiceUptimeComponent,
    StatusComponent,
    TuiAvatar,
    TuiCell,
    TuiTitle,
  ],
})
export class ServiceTileComponent {
  readonly pkg = input.required<PackageDataEntry>({ alias: 'appServiceTile' })
  readonly depErrors = input<PkgDependencyErrors>({})

  protected readonly manifest = computed(() => getManifest(this.pkg()))
  protected readonly hasError = computed(() =>
    Object.values(this.depErrors()).some(Boolean),
  )
}
