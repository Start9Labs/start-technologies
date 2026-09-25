import { DatePipe } from '@angular/common'
import { Component, inject, input, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import { MarketplacePkg } from '@start9labs/marketplace'
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
import { MarketplaceAlertsService } from '../marketplace/services/alerts.service'
import UpdatesComponent from './updates.component'

@Component({
  selector: 'updates-item',
  template: `
    <tr (click)="expanded.set(!expanded())">
      <td>
        <div [style.gap.rem]="0.75" [style.padding-inline-end.rem]="1">
          <span
            tuiAvatar
            appearance="action-grayscale"
            size="s"
            [round]="false"
          >
            <img alt="" [src]="item().icon" />
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
          {{ 'Show more' | i18n }}
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
            iconStart="@tui.x"
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
        @if (error()) {
          <p class="g-negative">{{ error() }}</p>
        }
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
                [textContent]="'View listing' | i18n"
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

    div {
      display: flex;
      align-items: center;
      gap: 0.25rem;
      white-space: nowrap;
    }

    tr:first-child {
      min-height: var(--tui-height-l);
      word-break: break-word;
      clip-path: inset(0 round var(--tui-radius-s));
      cursor: pointer;
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
      }

      td[colspan]:only-child {
        padding: 0 0.5rem;
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
    RouterLink,
    TuiExpand,
    TuiButton,
    TuiButtonLoading,
    TuiChevron,
    TuiAvatar,
    TuiLink,
    TuiIcon,
    TuiProgressCircle,
    TuiTitle,
    TuiFade,
    LocalizePipe,
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    DatePipe,
    InstallingProgressPipe,
    i18nPipe,
  ],
})
export class UpdatesItemComponent {
  private readonly service = inject(MarketplaceService)
  private readonly dialog = inject(DialogService)
  private readonly hiddenUpdates = inject(HiddenUpdatesService)
  private readonly alerts = inject(MarketplaceAlertsService)

  readonly parent = inject(UpdatesComponent)
  readonly expanded = signal(false)
  readonly error = signal('')
  readonly ready = signal(true)

  readonly item = input.required<MarketplacePkg>()
  readonly local =
    input.required<PackageDataEntry<InstalledState | UpdatingState>>()
  readonly pending = input.required<boolean>()

  async update() {
    const { id, version, preDownloadAlert } = this.item()
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
      await this.service.installPackage(id, version, url)
    } catch (e: any) {
      this.error.set(e.message)
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
