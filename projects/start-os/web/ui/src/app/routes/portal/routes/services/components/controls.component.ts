import { AsyncPipe } from '@angular/common'
import { Component, computed, DOCUMENT, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDataList, TuiDropdown } from '@taiga-ui/core'
import { TuiChevron } from '@taiga-ui/kit'
import { map } from 'rxjs'
import { ControlsService } from 'src/app/services/controls.service'
import { DepErrorService } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { PrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'
import { InterfaceService } from '../../../components/interfaces/interface.service'

@Component({
  selector: 'service-controls',
  template: `
    @if (['running', 'starting', 'restarting'].includes(status()!)) {
      <button
        tuiButton
        appearance="primary-destructive"
        iconStart="@tui.square"
        (click)="controls.stop(manifest())"
      >
        {{ 'Stop' | i18n }}
      </button>

      @if (status() === 'running') {
        <button
          tuiButton
          iconStart="@tui.rotate-cw"
          (click)="controls.restart(manifest().id)"
        >
          {{ 'Restart' | i18n }}
        </button>

        @if (interfaces().length > 1) {
          <button
            tuiButton
            appearance="primary-grayscale"
            tuiChevron
            tuiDropdownAuto
            tuiDropdownLimitWidth="min"
            [disabled]="!hasAnyHref()"
            [tuiDropdown]="content"
          >
            {{ 'Open UI' | i18n }}
          </button>
          <ng-template #content let-close>
            <tui-data-list>
              @for (i of interfaces(); track $index) {
                <a
                  tuiOption
                  target="_blank"
                  rel="noreferrer"
                  iconEnd="@tui.external-link"
                  [attr.href]="getHref(i)"
                  [class.disabled]="!getHref(i)"
                  (click)="close()"
                >
                  {{ i.name }}
                </a>
              }
            </tui-data-list>
          </ng-template>
        } @else if (interfaces()[0]; as ui) {
          <button
            tuiButton
            appearance="primary-grayscale"
            iconStart="@tui.external-link"
            [disabled]="!getHref(ui)"
            (click)="openUI(ui)"
          >
            {{ 'Open UI' | i18n }}
          </button>
        }
      }
    }

    @if (status() === 'stopped') {
      @let unmet = hasUnmet() | async;
      <button
        tuiButton
        iconStart="@tui.play"
        (click)="controls.start(manifest(), !!unmet)"
      >
        {{ 'Start' | i18n }}
      </button>
    }
  `,
  styles: `
    :host {
      width: 100%;
      max-width: 18rem;
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
      justify-content: center;
      margin-block-start: 1rem;

      &:nth-child(3) {
        grid-row: span 2;
      }
    }

    [tuiButton] {
      flex: 1;
      min-width: fit-content;
    }

    :host-context(tui-root._mobile) {
      display: flex;
      margin: 0;

      [tuiButton] {
        font-size: 0;
        gap: 0;
        border-radius: 100%;
      }
    }
  `,
  imports: [
    TuiButton,
    i18nPipe,
    AsyncPipe,
    TuiDataList,
    TuiDropdown,
    TuiChevron,
  ],
})
export class ServiceControlsComponent {
  private readonly errors = inject(DepErrorService)
  private readonly interfaceService = inject(InterfaceService)
  private readonly document = inject(DOCUMENT)

  readonly pkg = input.required<PackageDataEntry>()
  readonly status = input<PrimaryStatus>()
  readonly manifest = computed(() => getManifest(this.pkg()))
  readonly controls = inject(ControlsService)

  readonly hasUnmet = computed(() =>
    this.errors.getPkgDepErrors$(this.manifest().id).pipe(
      map(errors =>
        Object.keys(this.pkg().currentDependencies)
          .map(id => errors[id])
          .some(Boolean),
      ),
    ),
  )

  readonly interfaces = computed(() =>
    Object.values(this.pkg().hosts)
      .flatMap(host => Object.values(host.bindings))
      .flatMap(binding => Object.values(binding.interfaces))
      .filter(
        i =>
          i.type === 'ui' &&
          (i.addressInfo.scheme === 'http' ||
            i.addressInfo.sslScheme === 'https'),
      ),
  )

  readonly hasAnyHref = computed(() =>
    this.interfaces().some(i => !!this.getHref(i)),
  )

  getHref(ui: T.ServiceInterface): string {
    const host = this.pkg().hosts[ui.addressInfo.hostId]
    if (!host) return ''
    return this.interfaceService.launchableAddress(ui, host)
  }

  openUI(ui: T.ServiceInterface) {
    const href = this.getHref(ui)
    if (href) {
      this.document.defaultView?.open(href, '_blank', 'noreferrer')
    }
  }
}
