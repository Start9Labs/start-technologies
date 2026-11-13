import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  RegistryConnectionStatus,
  StoreIconDirective,
} from '@start9labs/marketplace'
import type { T } from '@start9labs/start-core'
import {
  defaultRegistries,
  DialogService,
  DocsLinkDirective,
  i18nKey,
  i18nPipe,
  registryUrl,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiDataList, TuiDropdown, TuiLoader } from '@taiga-ui/core'
import { TuiAvatar, TuiBadge } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { TRUSTED_KEYS_NOTICE } from 'src/app/components/trusted-keys/notice'
import { BackDirective } from 'src/app/routes/portal/components/back'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import {
  RegistryRefresh,
  RegistryRetry,
} from 'src/app/routes/portal/components/registry-retry'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { TRUSTED_KEYS_LINK } from '../../system.const'

interface Row extends Pick<T.RegistryCache, 'status' | 'error'> {
  url: string
  name: string
  os: boolean
  builtin: boolean
}

@Component({
  template: `
    <ng-container *title>
      <div>
        <button tuiIconButton appBack iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </button>
        {{ 'Registries' | i18n }}
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Registries' | i18n }}
        <a
          tuiIconButton
          docsLink
          path="/start-os/marketplace.html"
          size="xs"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button tuiButton size="xs" iconStart="@tui.plus" (click)="add()">
          {{ 'Add registry' | i18n }}
        </button>
      </header>
      @if (rows(); as rows) {
        <table [appTable]="[null, 'Name', 'URL', 'Status', null]">
          @for (row of rows; track row.url) {
            <tr>
              <td [style.width.rem]="3">
                <span tuiAvatar size="s"><img [storeIcon]="row.url" /></span>
              </td>
              <td class="name">
                <span>{{ row.name }}</span>
                @if (row.os) {
                  <span tuiBadge appearance="primary">
                    {{ 'StartOS updates' | i18n }}
                  </span>
                }
              </td>
              <td class="url">{{ row.url }}</td>
              <td class="status">
                <marketplace-registry-status
                  [status]="row.status"
                  [url]="row.url"
                  [error]="row.error"
                />
                @if (row.status === 'error') {
                  <button [registryRetry]="row.url" size="xs"></button>
                }
              </td>
              <td>
                <button
                  tuiIconButton
                  tuiDropdown
                  tuiDropdownAuto
                  tuiDropdownAlign="end"
                  size="s"
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                  [disabled]="saving()"
                >
                  {{ 'More' | i18n }} {{ row.name }}
                  <tui-data-list *tuiDropdown="let close" (click)="close()">
                    <button tuiOption [registryRefresh]="row.url">
                      {{ 'Refresh registry' | i18n }}
                    </button>
                    <button tuiOption (click)="manageKeys(row.url)">
                      {{ 'Manage keys' | i18n }}
                    </button>
                    @if (!row.os) {
                      <button tuiOption (click)="useForUpdates(row.url)">
                        {{ 'Use for StartOS updates' | i18n }}
                      </button>
                    }
                    @if (!row.builtin) {
                      <button
                        tuiOption
                        class="g-negative"
                        (click)="remove(row)"
                      >
                        {{ 'Remove' | i18n }}
                      </button>
                    }
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="5">{{ 'No registries connected' | i18n }}</td>
            </tr>
          }
        </table>
      } @else {
        <tui-loader />
      }
    </section>
  `,
  styles: `
    header button {
      margin-inline-start: auto;
    }

    [tuiBadge] {
      margin-inline-start: 0.5rem;
    }

    td:last-child {
      text-align: end;
    }

    :host-context(tui-root._mobile) {
      tr {
        grid-template-columns: auto 1fr auto;
        column-gap: 0.75rem;
      }

      td:first-child {
        grid-area: 1 / 1 / 4;
        align-content: center;
      }

      .url {
        grid-area: 2 / 2;
      }

      .status {
        grid-area: 3 / 2;
      }

      .name {
        grid-area: 1 / 2;
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.25rem 0.5rem;
        font: var(--tui-typography-body-m);
        font-weight: bold;
        color: var(--tui-text-primary);
      }

      [tuiBadge] {
        margin: 0;
      }

      td:last-child {
        grid-area: 1 / 3 / 4;
        align-content: center;
      }
    }
  `,
  imports: [
    RegistryConnectionStatus,
    RegistryRefresh,
    RegistryRetry,
    BackDirective,
    DocsLinkDirective,
    StoreIconDirective,
    TableComponent,
    TitleDirective,
    TuiAvatar,
    TuiBadge,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiLoader,
    i18nPipe,
  ],
})
export default class Registries {
  private readonly api = inject(ApiService)
  private readonly marketplace = inject(MarketplaceService)
  private readonly os = inject(OSService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly router = inject(Router)
  private readonly registries = toSignal(this.marketplace.registries$)
  private readonly osRegistry = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('serverInfo', 'osRegistry'),
  )

  protected readonly saving = signal(false)
  protected readonly rows = computed(() =>
    this.registries()?.map<Row>(({ url, name, status, error }) => ({
      url,
      name,
      status,
      error,
      os: sameUrl(url, this.osRegistry()),
      builtin: Object.values(defaultRegistries).some(known =>
        sameUrl(url, known),
      ),
    })),
  )

  protected async add(): Promise<void> {
    const rawUrl = await firstValueFrom(
      this.dialogs
        .openPrompt<string>({
          label: 'Add registry',
          data: {
            message: 'The domain or URL of the custom registry',
            label: 'URL',
            placeholder: 'e.g. registry.example.com',
            buttonText: 'Add',
          },
        })
        .pipe(defaultIfEmpty('')),
    )

    if (!rawUrl) return

    let added: string[] = []

    await this.save(async () => {
      added = await this.marketplace.connectRegistry(registryUrl(rawUrl))
    }, 'Validating registry')

    if (added.length) {
      await this.notify(
        'Registry added',
        'These keys from the registry are now trusted for packages.',
        added,
      )
    }
  }

  protected manageKeys(url: string): void {
    this.router.navigate(TRUSTED_KEYS_LINK, { queryParams: { registry: url } })
  }

  protected async useForUpdates(url: string): Promise<void> {
    let publishes = false

    const checked = await this.save(async () => {
      publishes = await this.os.publishesUpdates(url)
    }, 'Checking registry')

    if (!checked) return

    if (!publishes) {
      await firstValueFrom(
        this.dialogs
          .openAlert('This registry does not publish StartOS updates.', {
            label: 'StartOS updates unavailable',
          })
          .pipe(defaultIfEmpty(null)),
      )
      return
    }

    if (!sameUrl(url, defaultRegistries.start9)) {
      const confirmed = await firstValueFrom(
        this.dialogs
          .openConfirm({
            label: 'Warning',
            data: {
              content:
                'Only Start9 publishes official StartOS updates. An update from another registry replaces the operating system on this server, so use one only if you fully trust whoever runs it.',
              yes: 'Use for StartOS updates',
              no: 'Cancel',
            },
          })
          .pipe(defaultIfEmpty(false)),
      )

      if (!confirmed) return
    }

    let enabled: string[] = []
    const saved = await this.save(async () => {
      enabled = await this.marketplace.setOsRegistry(url)
    }, 'Saving')

    if (saved) {
      await this.notify(
        'StartOS updates',
        enabled.length
          ? 'StartOS updates now come from this registry. These keys are now trusted for StartOS updates.'
          : 'StartOS updates now come from this registry.',
        enabled,
      )
    }
  }

  protected async remove({ url, os }: Row): Promise<void> {
    if (os) {
      await firstValueFrom(
        this.dialogs
          .openAlert(
            'This registry provides StartOS updates. Choose another registry for StartOS updates before removing it.',
            { label: 'Registry in use' },
          )
          .pipe(defaultIfEmpty(null)),
      )
      return
    }

    const confirmed = await firstValueFrom(
      this.dialogs
        .openConfirm({
          label: 'Remove registry?',
          data: { yes: 'Remove', no: 'Cancel' },
        })
        .pipe(defaultIfEmpty(false)),
    )

    if (!confirmed) return

    let orphaned: string[] = []

    await this.save(async () => {
      orphaned = await this.marketplace.removeRegistry(url)
    }, 'Removing')

    if (
      orphaned.length &&
      (await this.notify(
        'Remove unused keys?',
        'No connected registry offers these keys anymore. Remove them from Trusted Keys?',
        orphaned,
        'Keep',
      ))
    ) {
      await this.save(async () => {
        await this.api.rpc.server.trustedKeys.removeMany({ ids: orphaned })
      }, 'Removing')
    }
  }

  private async save(task: () => Promise<void>, label: i18nKey) {
    this.saving.set(true)
    const done = await this.tasks.run(task, label)
    this.saving.set(false)

    return done
  }

  private notify(
    label: i18nKey,
    message: i18nKey,
    ids: readonly string[],
    no?: i18nKey,
  ): Promise<boolean> {
    return firstValueFrom(
      this.dialogs
        .openComponent<boolean>(TRUSTED_KEYS_NOTICE, {
          label,
          data: { message, ids, yes: no ? 'Remove keys' : 'Ok', no },
        })
        .pipe(defaultIfEmpty(false)),
    )
  }
}
