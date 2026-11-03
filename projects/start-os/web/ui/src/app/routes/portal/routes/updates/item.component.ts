import { DatePipe } from '@angular/common'
import {
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router } from '@angular/router'
import {
  MarketplaceIconDirective,
  MarketplacePkg,
  MarketplacePreviewComponent,
  matchingTrustedKeys,
  packageSigners,
} from '@start9labs/marketplace'
import { DialogService, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiButtonX,
  TuiIcon,
  TuiPopup,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiButtonLoading,
  TuiDrawer,
  TuiFade,
  TuiProgressCircle,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { debounceTime, filter } from 'rxjs'
import { InstallingProgressPipe } from 'src/app/routes/portal/routes/services/pipes/install-progress.pipe'
import { HiddenUpdatesService } from 'src/app/services/hidden-updates.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import {
  InstalledState,
  PackageDataEntry,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { MarketplaceControlsComponent } from '../marketplace/components/controls.component'
import { MarketplaceAlertsService } from '../marketplace/services/alerts.service'
import UpdatesComponent from './updates.component'

@Component({
  selector: 'updates-item',
  template: `
    <tr [class._note]="error() || !trusted().length" (click)="toggle(true)">
      <td>
        <div [style.gap.rem]="0.75" [style.padding-inline-end.rem]="1">
          <span
            tuiAvatar
            appearance="action-grayscale"
            size="s"
            [round]="false"
            [tuiSkeleton]="icon.loading()"
          >
            <img
              #icon="marketplaceIcon"
              [marketplaceIcon]="item()"
              [registry]="parent.current()?.url"
            />
          </span>
          <span tuiTitle [style.margin]="'-0.125rem 0 0'">
            <b tuiFade>{{ item().title }}</b>
            <span tuiSubtitle tuiFade class="mobile">
              <span class="g-secondary">
                {{ local().stateInfo.manifest.version }}
              </span>
              <tui-icon icon="@tui.arrow-right" />
              <span class="g-positive">{{ item().version }}</span>
            </span>
          </span>
        </div>
        @if (error(); as error) {
          <p class="note g-negative">{{ error }}</p>
        } @else if (!trusted().length) {
          <p class="note g-warning">
            {{
              "This service package isn't signed by anyone you trust." | i18n
            }}
          </p>
        }
        <tui-drawer
          *tuiPopup="open()"
          [overlay]="true"
          (click.self)="toggle(false)"
        >
          <marketplace-preview
            [pkgId]="item().id"
            [pkg]="item()"
            [controls]="controls"
          >
            <button
              tuiButtonX
              [tuiAppearanceFocus]="false"
              (click)="toggle(false)"
            >
              {{ 'Close' | i18n }}
            </button>
          </marketplace-preview>
        </tui-drawer>
        <ng-template #controls let-pkg>
          <marketplace-controls [pkg]="pkg" />
        </ng-template>
      </td>
      <td class="desktop">
        <div>
          <span class="g-secondary">
            {{ local().stateInfo.manifest.version }}
          </span>
          <tui-icon icon="@tui.arrow-right" />
          <span class="g-positive">{{ item().version }}</span>
        </div>
      </td>
      <td class="desktop">{{ item().gitHash }}</td>
      <td class="desktop">
        {{ item().s9pks[0]?.[1]?.publishedAt | date }}
      </td>
      <td>
        @if (local().stateInfo.state === 'updating') {
          <tui-progress-circle
            size="xs"
            [max]="100"
            [value]="
              (local().stateInfo.installingInfo?.progress?.overall
                | installingProgress) || 0
            "
          />
        } @else if (pending()) {
          <span class="g-secondary pending">
            {{ 'Finding suitable version...' | i18n }}
          </span>
        } @else {
          <button
            tuiIconButton
            size="s"
            appearance="secondary"
            iconStart="@tui.eye-off"
            (click.stop)="hide()"
          >
            {{ 'Hide' | i18n }}
          </button>
          <button
            tuiIconButton
            size="s"
            iconStart="@tui.download"
            [loading]="!ready()"
            [appearance]="error() ? 'destructive' : 'primary'"
            (click.stop)="update()"
          >
            {{ error() ? ('Retry' | i18n) : ('Update' | i18n) }}
          </button>
        }
      </td>
    </tr>
  `,
  styles: `
    :host {
      display: contents;
    }

    tui-icon {
      font-size: 1rem;
    }

    tui-progress-circle {
      display: inline-flex;
      vertical-align: middle;
    }

    div {
      display: flex;
      align-items: center;
      gap: 0.25rem;
      white-space: nowrap;
    }

    tr {
      position: relative;
      min-height: var(--tui-height-l);
      word-break: break-word;
      clip-path: inset(0 round var(--tui-radius-s));
      cursor: pointer;

      &._note td {
        padding-block-end: 2.125rem;

        &:first-child {
          position: static;
        }
      }
    }

    .note {
      position: absolute;
      z-index: 1;
      inset-inline: 3.5rem 0.75rem;
      inset-block-end: 0.875rem;
      margin: 0;
      white-space: normal;
    }

    td {
      min-width: 0;
      vertical-align: middle;

      &:first-child {
        white-space: nowrap;
      }

      &:last-child {
        white-space: nowrap;
        text-align: right;

        [tuiIconButton] + [tuiIconButton] {
          margin-inline-start: 0.375rem;
        }
      }
    }

    .pending {
      margin-right: 0.5rem;
      font-style: italic;
    }

    .mobile {
      display: none;
    }

    tui-drawer {
      top: 0;
      width: 28rem;
      border-radius: 0;
    }

    :host-context(tui-root._mobile) {
      tr {
        display: grid;
        grid-template-columns: 1fr min-content;
        align-items: center;
        padding: 0.5rem 0;

        &._note {
          padding-block-end: 0.875rem;

          td {
            padding-block-end: 0;
          }
        }
      }

      .note {
        position: static;
        margin: 0.25rem 0 0 2.75rem;
        font: var(--tui-typography-body-s);
        white-space: normal;
      }

      .desktop {
        display: none;
      }

      .mobile {
        display: flex;
        gap: 0.25rem;

        [tuiSubtitle] {
          color: var(--tui-text-secondary);
        }
      }
    }
  `,
  imports: [
    DatePipe,
    InstallingProgressPipe,
    MarketplaceControlsComponent,
    MarketplaceIconDirective,
    MarketplacePreviewComponent,
    TuiAvatar,
    TuiButton,
    TuiButtonLoading,
    TuiButtonX,
    TuiDrawer,
    TuiFade,
    TuiIcon,
    TuiPopup,
    TuiProgressCircle,
    TuiSkeleton,
    TuiTitle,
    i18nPipe,
  ],
})
export class UpdatesItemComponent {
  private readonly service = inject(MarketplaceService)
  private readonly trust = inject(TrustedKeysService)
  private readonly dialog = inject(DialogService)
  private readonly hiddenUpdates = inject(HiddenUpdatesService)
  private readonly alerts = inject(MarketplaceAlertsService)
  private readonly router = inject(Router)
  private readonly params = toSignal(
    inject(ActivatedRoute).queryParamMap.pipe(debounceTime(100)),
  )

  readonly parent = inject(UpdatesComponent)
  readonly error = signal('')
  readonly ready = signal(true)

  readonly item = input.required<MarketplacePkg>()
  readonly local =
    input.required<PackageDataEntry<InstalledState | UpdatingState>>()
  readonly pending = input.required<boolean>()

  protected readonly trusted = computed(() =>
    matchingTrustedKeys(
      packageSigners(this.item()),
      this.trust.keys() ?? {},
      'marketplace',
    ),
  )
  protected readonly open = computed(
    () =>
      this.params()?.get('id') === this.item().id &&
      this.params()?.get('flavor') === this.item().flavor,
  )

  constructor() {
    effect(() => {
      const url = this.parent.current()?.url
      if (this.open() && url) this.service.currentRegistryUrl$.next(url)
    })
  }

  toggle(open: boolean) {
    this.router.navigate([], {
      queryParams: {
        id: open ? this.item().id : null,
        flavor: open ? this.item().flavor : null,
      },
      queryParamsHandling: 'merge',
    })
  }

  async update() {
    const trust = !this.trusted().length
    if (trust && !(await this.alerts.alertUntrusted())) return

    const pkg = this.item()
    const { preDownloadAlert } = pkg
    const url = this.parent.current()?.url || ''

    if (
      !(await this.alerts.alertPreDownload(
        preDownloadAlert,
        this.local().stateInfo.manifest.version,
      ))
    ) {
      return
    }

    this.ready.set(false)

    try {
      await this.service.installPackage(pkg, url, trust)
    } catch (e) {
      this.error.set(e instanceof Error ? e.message : String(e))
    } finally {
      this.ready.set(true)
    }
  }

  hide() {
    this.dialog
      .openConfirm({
        label: 'Hide Update?',
        size: 's',
        data: {
          content:
            'This update will be hidden. You can still install this version from the Marketplace.',
          yes: 'Hide',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        const { id, version } = this.item()
        this.hiddenUpdates.hide(id, version)
      })
  }
}
