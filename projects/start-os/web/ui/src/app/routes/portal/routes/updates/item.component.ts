import { DatePipe } from '@angular/common'
import { Component, computed, inject, input, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  MarketplaceIconDirective,
  MarketplacePkg,
  matchingTrustedKeys,
  MarketplaceSignerChipsComponent,
  packageSigners,
} from '@start9labs/marketplace'
import {
  DialogService,
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import {
  TuiButton,
  TuiExpand,
  TuiIcon,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import {
  TuiAvatar,
  TuiButtonLoading,
  TuiChevron,
  TuiFade,
  TuiProgressCircle,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { filter } from 'rxjs'
import { InstallingProgressPipe } from 'src/app/routes/portal/routes/services/pipes/install-progress.pipe'
import { HiddenUpdatesService } from 'src/app/services/hidden-updates.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import {
  InstalledState,
  PackageDataEntry,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { MarketplaceAlertsService } from '../marketplace/services/alerts.service'
import UpdatesComponent from './updates.component'

@Component({
  selector: 'updates-item',
  template: `
    <tr
      [class._note]="error() || !trusted().length"
      (click)="expanded.set(!expanded())"
    >
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
        <button
          tuiIconButton
          size="m"
          appearance="icon"
          [tuiChevron]="expanded()"
        >
          {{ 'Details' | i18n }}
        </button>
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
    <tr>
      <td colspan="5">
        <tui-expand [expanded]="expanded()">
          <p tuiTitle class="mobile">
            <b>{{ 'Package Hash' | i18n }}</b>
            <span tuiSubtitle>{{ item().gitHash }}</span>
          </p>
          <p tuiTitle class="mobile">
            <b>{{ 'Published' | i18n }}</b>
            <span tuiSubtitle>
              {{ item().s9pks[0]?.[1]?.publishedAt | date }}
            </span>
          </p>
          <p tuiTitle>
            <b>{{ 'Signed by' | i18n }}</b>
            <marketplace-signer-chips [keys]="signers()" />
          </p>
          <p tuiTitle>
            <span>
              <b>{{ 'Release notes' | i18n }}</b>
              (
              <a
                tuiLink
                iconEnd="@tui.external-link"
                routerLink="/marketplace"
                [queryParams]="{
                  registry: parent.current()?.url,
                  id: item().id,
                  flavor: item().flavor,
                }"
                [textContent]="'View in Marketplace' | i18n"
              ></a>
              )
            </span>
          </p>
          <p
            safeLinks
            [innerHTML]="item().releaseNotes | localize | markdown | dompurify"
          ></p>
        </tui-expand>
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

    td > div {
      display: flex;
      align-items: center;
      gap: 0.25rem;
      white-space: nowrap;
    }

    tr:first-child {
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

      &[colspan]:only-child {
        padding: 0 3rem;
        text-align: left;
        white-space: normal;
        overflow-wrap: anywhere;
      }
    }

    [tuiTitle] {
      margin: 1rem 0;
    }

    .pending {
      margin-right: 0.5rem;
      font-style: italic;
    }

    .mobile {
      display: none;
    }

    :host-context(tui-root._mobile) {
      tr:first-child {
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

      td[colspan]:only-child {
        padding: 0 0.5rem;
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
    LocalizePipe,
    MarkdownPipe,
    MarketplaceIconDirective,
    MarketplaceSignerChipsComponent,
    NgDompurifyPipe,
    RouterLink,
    SafeLinksDirective,
    TuiAvatar,
    TuiButton,
    TuiButtonLoading,
    TuiChevron,
    TuiExpand,
    TuiFade,
    TuiIcon,
    TuiLink,
    TuiProgressCircle,
    TuiSkeleton,
    TuiTitle,
    i18nPipe,
  ],
})
export class UpdatesItemComponent {
  private readonly service = inject(MarketplaceService)
  private readonly dialog = inject(DialogService)
  private readonly hiddenUpdates = inject(HiddenUpdatesService)
  private readonly alerts = inject(MarketplaceAlertsService)

  private readonly trust = inject(TrustedKeysService)
  readonly parent = inject(UpdatesComponent)
  readonly expanded = signal(false)
  readonly error = signal('')
  readonly ready = signal(true)

  readonly item = input.required<MarketplacePkg>()
  readonly local =
    input.required<PackageDataEntry<InstalledState | UpdatingState>>()
  readonly pending = input.required<boolean>()

  protected readonly signers = computed(() => packageSigners(this.item()))
  protected readonly trusted = computed(() =>
    matchingTrustedKeys(this.signers(), this.trust.keys() ?? {}, 'marketplace'),
  )

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
