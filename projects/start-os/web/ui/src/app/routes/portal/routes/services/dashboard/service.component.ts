import { Component, computed, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiAvatar } from '@taiga-ui/kit'
import { ServiceUptimeComponent } from 'src/app/routes/portal/routes/services/components/uptime.component'
import { PkgDependencyErrors } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { StatusComponent } from './status.component'

@Component({
  selector: 'tr[appService]',
  template: `
    <td [style.width.rem]="3">
      <i tuiAvatar size="s" [round]="false">
        <img alt="logo" [src]="pkg().icon" />
      </i>
    </td>
    <td class="title">
      <a [routerLink]="'/services/' + manifest().id">{{ manifest().title }}</a>
    </td>
    <td class="status">
      <app-status [pkg]="pkg()" [hasDepErrors]="hasError()" />
    </td>
    <td class="version">{{ manifest().version }}</td>
    <td class="uptime">
      @if (pkg().statusInfo.started; as started) {
        <service-uptime [started]="started" />
      } @else {
        -
      }
    </td>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    :host {
      @include taiga.transition(background);
      cursor: pointer;

      &:hover {
        background: var(--tui-background-neutral-1);
      }
    }

    td::before {
      display: none;
    }

    a {
      color: var(--tui-text-primary);
      font-weight: bold;
    }

    .title {
      width: 21rem;
    }

    .status {
      width: 21rem;
    }

    .uptime {
      width: 13rem;
    }

    .version {
      width: 21rem;
    }
  `,
  imports: [RouterLink, ServiceUptimeComponent, StatusComponent, TuiAvatar],
})
export class ServiceComponent {
  readonly pkg = input.required<PackageDataEntry>()
  readonly depErrors = input<PkgDependencyErrors>({})

  readonly manifest = computed(() => getManifest(this.pkg()))
  readonly hasError = computed(() =>
    Object.values(this.depErrors()).some(Boolean),
  )
}
