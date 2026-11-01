import { CommonModule } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute } from '@angular/router'
import { isEmptyObject } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { map, of } from 'rxjs'
import { ConnectionService } from 'src/app/services/connection.service'
import { DepErrorService } from 'src/app/services/dep-error.service'
import {
  DataModel,
  PackageDataEntry,
} from 'src/app/services/patch-db/data-model'
import { getInstalledPrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { ServiceTasksComponent } from '../components/tasks.component'
import { ServiceControlsComponent } from '../components/controls.component'
import { ServiceDependenciesComponent } from '../components/dependencies.component'
import { ServiceErrorComponent } from '../components/error.component'
import { ServiceHealthChecksComponent } from '../components/health-checks.component'
import { ServiceInstallProgressComponent } from '../components/progress.component'
import { ServiceStatusComponent } from '../components/status.component'
import { ServiceUptimeComponent } from '../components/uptime.component'

@Component({
  template: `
    @if (pkg(); as pkg) {
      <ng-template #controls>
        @if (connected() && installed()) {
          <service-controls [pkg]="pkg" [status]="status()" />
        }
      </ng-template>
      @if (pkg.statusInfo.error) {
        <service-error [pkg]="pkg" />
        <ng-container [ngTemplateOutlet]="controls" />
      } @else if (installing()) {
        <service-install-progress [pkg]="pkg" />
      } @else if (installed()) {
        <service-status [connected]="!!connected()" [pkg]="pkg">
          <ng-container [ngTemplateOutlet]="controls" />
        </service-status>

        @if (status() !== 'backing-up') {
          <service-health-checks [checks]="health()" />
          <service-uptime class="g-card" [started]="pkg.statusInfo.started" />

          <service-tasks [pkg]="pkg" [services]="services() || {}" />

          @if (errors() | async; as errors) {
            <service-dependencies
              [pkg]="pkg"
              [services]="services()"
              [errors]="errors"
            />
          }
        }
      } @else if (removing()) {
        <service-status [connected]="!!connected()" [pkg]="pkg" />
      }
    }
  `,
  styles: `
    :host {
      display: grid;
      grid-template-columns: repeat(10, 1fr);
      grid-auto-rows: max-content;
      gap: 1rem;
    }

    small {
      font-weight: normal;
      text-transform: uppercase;
    }

    :host-context(tui-root._mobile) {
      grid-template-columns: 1fr;

      > * {
        grid-column: span 1;
      }

      service-uptime {
        display: none;
      }
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    CommonModule,
    ServiceInstallProgressComponent,
    ServiceStatusComponent,
    ServiceControlsComponent,
    ServiceHealthChecksComponent,
    ServiceDependenciesComponent,
    ServiceErrorComponent,
    ServiceTasksComponent,
    ServiceUptimeComponent,
  ],
})
export class ServiceRoute {
  private readonly errorService = inject(DepErrorService)
  protected readonly connected = toSignal(inject(ConnectionService))

  protected readonly id = toSignal(
    inject(ActivatedRoute).paramMap.pipe(map(params => params.get('pkgId'))),
  )

  protected readonly services = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('packageData'),
    { initialValue: {} as Record<string, PackageDataEntry> },
  )

  protected readonly errors = computed((id = this.id()) =>
    id ? this.errorService.getPkgDepErrors$(id) : of({}),
  )

  protected readonly pkg = computed(() => this.services()[this.id() || ''])

  protected readonly health = computed((pkg = this.pkg()) =>
    pkg ? toHealthCheck(pkg.statusInfo) : [],
  )

  protected readonly status = computed((pkg = this.pkg()) =>
    pkg?.stateInfo.state === 'installed'
      ? getInstalledPrimaryStatus(pkg)
      : pkg?.stateInfo.state,
  )

  protected readonly installed = computed(
    () => this.pkg()?.stateInfo.state === 'installed',
  )

  protected readonly installing = computed(
    (state = this.status()) =>
      state === 'installing' || state === 'updating' || state === 'restoring',
  )

  protected readonly removing = computed(
    () => this.pkg()?.stateInfo.state === 'removing',
  )
}

function toHealthCheck(statusInfo: T.StatusInfo): T.NamedHealthCheckResult[] {
  return statusInfo.desired.main !== 'running' ||
    !statusInfo.started ||
    isEmptyObject(statusInfo.health)
    ? []
    : Object.values(statusInfo.health).filter(h => !!h)
}
