import { KeyValuePipe } from '@angular/common'
import { Component, inject, input, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  DialogService,
  i18nKey,
  i18nPipe,
  LocalizePipe,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiCell, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiButtonLoading, TuiSkeleton } from '@taiga-ui/kit'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PkgDependencyErrors } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import {
  dependencyCandidates,
  dependencyProviderChoices,
} from 'src/app/utils/dependency-provider'
import { getManifest } from 'src/app/utils/get-package-data'
import { ToManifestPipe } from '../../../pipes/to-manifest'
import {
  PROVIDER_SELECTION,
  ProviderSelectionData,
  ProviderSelectionValue,
} from './provider-selection'

@Component({
  selector: 'service-dependencies',
  template: `
    <header>{{ 'Dependencies' | i18n }}</header>
    @let services = this.services();
    @for (d of pkg().currentDependencies | keyvalue; track d.key) {
      @let binding = pkg().dependencyBindings?.[d.key];
      @let selection = pkg().dependencySelections?.[d.key];
      @let pinned = selection?.pin;
      @let choices = providerChoices(d.key);
      @let providerId = pinned ? selection?.providerId : binding?.providerId;
      @let provider = providerId ? services[providerId] : undefined;
      @let manifest = provider ? (provider | toManifest) : null;
      @let needsSelection = errors()[d.key]?.type === 'selectionRequired';
      <div tuiCell [class.error]="getError(d.key)">
        <span
          tuiAvatar
          appearance="action-grayscale"
          [round]="false"
          [tuiSkeleton]="icon.loading()"
        >
          <img
            #icon="pkgIcon"
            [pkgIcon]="provider || pinned || pkg()"
            [dependency]="provider || pinned ? undefined : d.key"
          />
        </span>
        <a
          tuiTitle
          [routerLink]="
            binding?.providerId
              ? ['..', binding!.providerId]
              : needsSelection
                ? null
                : ['/marketplace']
          "
          [attr.aria-disabled]="needsSelection || null"
          [queryParams]="
            binding?.providerId
              ? {}
              : {
                  provides: d.key,
                  requirePkg: d.key,
                  requireRange: d.value.versionRange,
                  providerId: pinned ? providerId : null,
                }
          "
        >
          {{ d.value.title || d.key | localize }}
          <span tuiSubtitle>{{ d.value.versionRange }}</span>
          @if (manifest || pinned) {
            <span tuiSubtitle>
              {{ 'Provided by' | i18n }}: {{ manifest?.title || pinned?.title }}
            </span>
          }
          @if (getError(d.key); as error) {
            <span tuiSubtitle class="g-warning">
              {{ error | i18n }}
              @if (getHealthCheckName(d.key); as name) {
                : {{ name }}
              }
            </span>
          } @else if (binding?.providerId) {
            <span tuiSubtitle class="g-positive">{{ 'Satisfied' | i18n }}</span>
          } @else {
            <span tuiSubtitle>{{ 'Loading' | i18n }}</span>
          }
        </a>
        @if (pinned && choices.length === 1) {
          <button
            tuiButton
            size="s"
            appearance="primary"
            iconStart="@tui.lock-open"
            [loading]="saving()"
            (click)="
              setProvider(d.key, { providerId: providerId!, locked: false })
            "
          >
            {{ 'Unlock' | i18n }}
          </button>
        } @else if (choices.length) {
          <button
            tuiButton
            size="s"
            appearance="primary"
            iconStart="@tui.shuffle"
            [loading]="saving()"
            (click)="switchProvider(d.key)"
          >
            {{ 'Switch' | i18n }}
          </button>
        }
      </div>
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
    TuiButton,
    TuiButtonLoading,
    TuiCell,
    TuiAvatar,
    TuiSkeleton,
    TuiTitle,
    PlaceholderComponent,
    i18nPipe,
    LocalizePipe,
    ToManifestPipe,
    PkgIconDirective,
  ],
})
export class ServiceDependenciesComponent {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  protected readonly saving = signal(false)

  public readonly pkg = input.required<PackageDataEntry>()
  public readonly services = input.required<Record<string, PackageDataEntry>>()
  public readonly errors = input.required<PkgDependencyErrors>()

  protected providerChoices(id: string) {
    const selection = this.pkg().dependencySelections?.[id]
    return dependencyProviderChoices(
      dependencyCandidates(this.pkg(), this.services(), id),
      selection?.pin ? selection.providerId : null,
    )
  }

  protected switchProvider(id: string) {
    const selection = this.pkg().dependencySelections?.[id]
    const data: ProviderSelectionData = {
      providerId: selection?.pin
        ? selection.providerId
        : (this.pkg().dependencyBindings?.[id]?.providerId ?? null),
      pin: selection?.pin ?? null,
      candidates: dependencyCandidates(this.pkg(), this.services(), id),
      services: this.services(),
    }
    this.dialogs
      .openComponent<ProviderSelectionValue>(PROVIDER_SELECTION, { data })
      .subscribe(value => void this.setProvider(id, value))
  }

  protected async setProvider(
    dependencyId: string,
    value: ProviderSelectionValue,
  ) {
    this.saving.set(true)
    try {
      await this.tasks.runInline(() =>
        this.api.rpc.package.selectDependencyProvider({
          id: getManifest(this.pkg()).id,
          dependencyId,
          ...value,
        }),
      )
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
