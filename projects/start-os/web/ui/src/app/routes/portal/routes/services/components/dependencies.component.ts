import { KeyValuePipe } from '@angular/common'
import { Component, inject, input, signal } from '@angular/core'
import { FormsModule, NgModel } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { i18nKey, i18nPipe, TaskService } from '@start9labs/shared'
import {
  TuiCell,
  TuiDataList,
  TuiIcon,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiChevron, TuiSelect, TuiSkeleton } from '@taiga-ui/kit'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PkgDependencyErrors } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { ToManifestPipe } from '../../../pipes/to-manifest'

@Component({
  selector: 'service-dependencies',
  template: `
    <header>{{ 'Dependencies' | i18n }}</header>
    @let services = this.services();
    @for (d of pkg().currentDependencies | keyvalue; track d.key) {
      @let binding = pkg().dependencyBindings?.[d.key];
      @let providerId = binding?.providerId;
      @let provider = providerId ? services[providerId] : undefined;
      @let manifest = provider ? (provider | toManifest) : null;
      <a
        tuiCell
        [routerLink]="providerId ? ['..', providerId] : ['/marketplace']"
        [queryParams]="
          providerId
            ? {}
            : {
                provides: d.key,
                requirePkg: d.key,
                requireRange: d.value.versionRange,
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
            [pkgIcon]="provider || pkg()"
            [dependency]="provider ? undefined : d.key"
          />
        </span>
        <span tuiTitle>
          {{ d.value.title || d.key }}
          <span tuiSubtitle>{{ d.key }} · {{ d.value.versionRange }}</span>
          @if (manifest) {
            <span tuiSubtitle>
              {{ 'Provided by' | i18n }}: {{ manifest.title }} ({{
                providerId
              }}) · {{ manifest.version }}
            </span>
          }
          @if (getError(d.key); as error) {
            <span tuiSubtitle class="g-warning">
              {{ error | i18n }}
              @if (getHealthCheckName(d.key); as name) {
                : {{ name }}
              }
            </span>
          } @else if (providerId) {
            <span tuiSubtitle class="g-positive">{{ 'Satisfied' | i18n }}</span>
          } @else {
            <span tuiSubtitle>{{ 'Loading' | i18n }}</span>
          }
        </span>
        <tui-icon icon="@tui.arrow-right" />
      </a>
      @if (binding) {
        <tui-textfield
          tuiChevron
          [tuiTextfieldCleaner]="false"
          [stringify]="stringify"
        >
          <label tuiLabel>
            {{ 'Provider' | i18n }}: {{ d.value.title || d.key }}
          </label>
          <input
            #selection="ngModel"
            tuiSelect
            [ngModel]="pkg().dependencySelections?.[d.key] || ''"
            [disabled]="saving()"
            (ngModelChange)="select(d.key, $event, selection)"
          />
          <tui-data-list *tuiDropdown>
            <button tuiOption [value]="''">{{ 'Automatic' | i18n }}</button>
            @for (
              candidate of binding.candidates | keyvalue;
              track candidate.key
            ) {
              <button tuiOption [value]="candidate.key">
                {{ candidate.key }} · {{ candidate.value.join(', ') }}
              </button>
            }
            @let preferred = pkg().dependencySelections?.[d.key];
            @if (preferred && !binding.candidates[preferred]) {
              <button tuiOption [value]="preferred" [disabled]="true">
                {{ preferred }} ({{ 'Unavailable' | i18n }})
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      }
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
    FormsModule,
    RouterLink,
    TuiCell,
    TuiAvatar,
    TuiSkeleton,
    TuiTitle,
    TuiIcon,
    TuiTextfield,
    TuiChevron,
    TuiSelect,
    TuiDataList,
    PlaceholderComponent,
    i18nPipe,
    ToManifestPipe,
    PkgIconDirective,
  ],
})
export class ServiceDependenciesComponent {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)
  protected readonly saving = signal(false)
  protected readonly stringify = (id: string) =>
    id || this.i18n.transform('Automatic')

  public readonly pkg = input.required<PackageDataEntry>()
  public readonly services = input.required<Record<string, PackageDataEntry>>()
  public readonly errors = input.required<PkgDependencyErrors>()

  protected async select(
    dependencyId: string,
    providerId: string,
    selection: NgModel,
  ) {
    this.saving.set(true)
    try {
      const saved = await this.tasks.runInline(() =>
        this.api.rpc.package.selectDependencyProvider({
          id: getManifest(this.pkg()).id,
          dependencyId,
          providerId: providerId || null,
        }),
      )
      if (!saved)
        selection.reset(this.pkg().dependencySelections?.[dependencyId] || '')
    } finally {
      this.saving.set(false)
    }
  }

  protected getError(id: string): i18nKey | undefined {
    switch (this.errors()[id]?.type) {
      case 'selectionRequired':
        return 'Select a dependency provider'
      case 'notInstalled':
        return 'Not installed'
      case 'noCompatibleProvider':
        return 'No compatible version available'
      case 'notRunning':
        return 'Not running'
      case 'taskRequired':
        return 'Task Required'
      case 'healthChecksFailed':
        return 'Required health check not passing'
      case 'transitive':
        return 'Dependency has a dependency issue'
      default:
        return undefined
    }
  }

  protected getHealthCheckName(id: string) {
    const error = this.errors()[id]
    return error?.type === 'healthChecksFailed' ? error.check?.name : undefined
  }
}
