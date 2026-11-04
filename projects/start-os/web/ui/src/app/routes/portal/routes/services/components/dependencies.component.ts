import { KeyValuePipe } from '@angular/common'
import { Component, inject, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { Exver, i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiIcon, TuiTitle, TuiCell } from '@taiga-ui/core'
import { TuiAvatar, TuiSkeleton } from '@taiga-ui/kit'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { PkgDependencyErrors } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ToManifestPipe } from '../../../pipes/to-manifest'

@Component({
  selector: 'service-dependencies',
  template: `
    <header>{{ 'Dependencies' | i18n }}</header>

    @let services = this.services();

    @for (d of pkg().currentDependencies | keyvalue; track $index) {
      @let manifest = services[d.key] ? (services[d.key]! | toManifest) : null;
      @let compatible =
        manifest && errors()[d.key]?.type !== 'incorrectVersion';
      <a
        tuiCell
        [routerLink]="compatible ? ['..', d.key] : ['/marketplace']"
        [queryParams]="
          compatible
            ? {}
            : {
                search: d.key,
                id: manifest ? d.key : null,
                requirePkg: d.key,
                requireRange: d.value.versionRange,
                flavor: manifest && exver.getFlavor(manifest.version),
              }
        "
        [class.error]="getError(d.key)"
      >
        <span
          tuiAvatar
          appearance="action-grayscale"
          [round]="false"
          [tuiSkeleton]="icon.loading()"
        >
          <img
            #icon="pkgIcon"
            [pkgIcon]="services[d.key] || pkg()"
            [dependency]="services[d.key] ? undefined : d.key"
          />
        </span>
        <span tuiTitle>
          {{ manifest?.title || d.value.title || d.key }}
          @if (getError(d.key); as error) {
            <span tuiSubtitle class="g-warning">
              {{ error | i18n }}
              @if (getHealthCheckName(d.key); as healthCheckName) {
                : {{ healthCheckName }}
              }
            </span>
          } @else {
            <span tuiSubtitle class="g-positive">{{ 'Satisfied' | i18n }}</span>
          }
          @if (services[d.key]?.stateInfo.manifest; as installedManifest) {
            <span tuiSubtitle>{{ installedManifest.version }}</span>
          }
        </span>
        <tui-icon icon="@tui.arrow-right" />
      </a>
    } @empty {
      <app-placeholder icon="@tui.boxes">
        {{ 'No dependencies' | i18n }}
      </app-placeholder>
    }
  `,
  styles: `
    :host {
      min-height: 12rem;
      grid-column: span 3;
    }

    .error {
      box-shadow: inset 1.25rem 0 0 -1rem var(--tui-status-warning);
    }
  `,
  host: { class: 'g-card' },
  imports: [
    KeyValuePipe,
    RouterLink,
    TuiCell,
    TuiAvatar,
    TuiSkeleton,
    TuiTitle,
    TuiIcon,
    PlaceholderComponent,
    i18nPipe,
    ToManifestPipe,
    PkgIconDirective,
  ],
})
export class ServiceDependenciesComponent {
  protected readonly exver = inject(Exver)

  pkg = input.required<PackageDataEntry>()
  services = input.required<Record<string, PackageDataEntry>>()
  errors = input.required<PkgDependencyErrors>()

  getError(id: string): i18nKey | undefined {
    const depError = this.errors()[id]

    if (!depError) {
      return undefined
    }

    switch (depError.type) {
      case 'notInstalled':
        return 'Not installed'
      case 'incorrectVersion':
        return 'Incompatible version'
      case 'notRunning':
        return 'Not running'
      case 'taskRequired':
        return 'Task Required'
      case 'healthChecksFailed':
        return 'Required health check not passing'
      case 'transitive':
        return 'Dependency has a dependency issue'
      default:
        return 'Unknown error'
    }
  }

  getHealthCheckName(id: string) {
    const depError = this.errors()[id]
    return depError?.type === 'healthChecksFailed'
      ? depError.check?.name
      : undefined
  }
}
