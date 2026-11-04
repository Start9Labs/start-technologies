import { Component, computed, inject, input, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { keyIdentities, keyType, TrustContext } from '@start9labs/marketplace'
import {
  DialogService,
  DocsLinkDirective,
  i18nKey,
  i18nPipe,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TUI_BREAKPOINT,
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiChip, TuiSwitch } from '@taiga-ui/kit'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { ADD_TRUSTED_KEY } from 'src/app/components/trusted-keys/form'
import { TRUSTED_KEYS_NOTICE } from 'src/app/components/trusted-keys/notice'
import { TRUST_PERMISSIONS } from 'src/app/components/trusted-keys/permissions'
import { BackDirective } from 'src/app/routes/portal/components/back'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { TitleDirective } from 'src/app/services/title.service'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'

@Component({
  template: `
    <ng-container *title>
      <div>
        <button tuiIconButton appBack iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </button>
        {{ 'Trusted Keys' | i18n }}
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Trusted Keys' | i18n }}
        <a
          tuiIconButton
          docsLink
          path="/start-os/trusted-keys.html"
          size="xs"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button tuiButton size="xs" iconStart="@tui.plus" (click)="add()">
          {{ 'Add key' | i18n }}
        </button>
      </header>
      @if (filter(); as filter) {
        <a
          tuiChip
          size="s"
          iconEnd="@tui.x"
          [routerLink]="[]"
          [queryParams]="{ registry: null }"
          [replaceUrl]="true"
        >
          {{ 'Linked to' | i18n }} {{ filter.name }}
        </a>
      }
      @if (rows(); as rows) {
        <table [appTable]="[]">
          <th>{{ 'Key' | i18n }}</th>
          <th>{{ 'Type' | i18n }}</th>
          @for (permission of permissions; track permission.context) {
            <th class="trust">{{ permission.title | i18n }}</th>
          }
          <th>{{ 'Linked registries' | i18n }}</th>
          <th></th>
          @for (row of rows; track row.id) {
            <tr>
              <td>
                <span tuiTitle>
                  <b>{{ row.key.name }}</b>
                  @if (row.identities[0]; as identity) {
                    <span tuiSubtitle>
                      {{ identity }}
                      @if (row.identities.length > 1) {
                        +{{ row.identities.length - 1 }}
                      }
                    </span>
                  }
                </span>
              </td>
              <td class="extra" [attr.data-label]="'Type' | i18n">
                {{ row.type }}
              </td>
              @if (breakpoint() !== 'mobile') {
                @for (permission of permissions; track permission.context) {
                  <td class="trust">
                    <tui-loader
                      size="xs"
                      [loading]="
                        changing()?.id === row.id &&
                        changing()?.context === permission.context
                      "
                    >
                      <input
                        tuiSwitch
                        type="checkbox"
                        size="s"
                        [showIcons]="false"
                        [disabled]="trust.saving()"
                        [ngModel]="row.key[permission.context]"
                        [attr.aria-label]="
                          (permission.label | i18n) + ': ' + row.key.name
                        "
                        (click.prevent)="
                          toggle(row.id, row.key, permission.context)
                        "
                      />
                    </tui-loader>
                  </td>
                }
              }
              <td class="extra" [attr.data-label]="'Linked registries' | i18n">
                @for (offer of row.offers; track offer.url) {
                  <div>{{ offer.name }}</div>
                } @empty {
                  —
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
                  [disabled]="trust.saving()"
                  [loading]="
                    breakpoint() === 'mobile' && changing()?.id === row.id
                  "
                >
                  {{ 'More' | i18n }} {{ row.key.name }}
                  <tui-data-list *tuiDropdown="let close" (click)="close()">
                    <button tuiOption (click)="trust.viewKey(row.id)">
                      {{ 'View details' | i18n }}
                    </button>
                    <button tuiOption (click)="trust.rename(row.id, row.key)">
                      {{ 'Edit label' | i18n }}
                    </button>
                    @if (breakpoint() === 'mobile') {
                      @for (
                        permission of permissions;
                        track permission.context
                      ) {
                        <button
                          tuiOption
                          [iconStart]="
                            row.key[permission.context]
                              ? '@tui.toggle-right'
                              : '@tui.toggle-left'
                          "
                          (click)="toggle(row.id, row.key, permission.context)"
                        >
                          {{
                            (row.key[permission.context]
                              ? permission.disable
                              : permission.enable
                            ) | i18n
                          }}
                        </button>
                      }
                    }
                    @if (row.key.removable) {
                      <button
                        tuiOption
                        class="g-negative"
                        (click)="trust.remove(row.id)"
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
              <td colspan="6">{{ 'No keys' | i18n }}</td>
            </tr>
          }
        </table>
      } @else {
        <tui-loader />
      }
    </section>
  `,
  styles: `
    header > button {
      margin-inline-start: auto;
    }

    [tuiChip] {
      margin: 0 0 1rem;
    }

    .trust {
      text-align: center;
    }

    td:last-child {
      text-align: end;
    }

    :host-context(tui-root._mobile) {
      .extra {
        display: none;
      }

      td:last-child {
        grid-area: 1 / 2;
        align-content: center;
      }
    }
  `,
  imports: [
    BackDirective,
    DocsLinkDirective,
    FormsModule,
    RouterLink,
    TableComponent,
    TitleDirective,
    TuiButton,
    TuiButtonLoading,
    TuiChip,
    TuiDataList,
    TuiDropdown,
    TuiLoader,
    TuiSwitch,
    TuiTitle,
    i18nPipe,
  ],
})
export default class TrustedKeys {
  private readonly dialogs = inject(DialogService)

  protected readonly trust = inject(TrustedKeysService)
  protected readonly breakpoint = inject(TUI_BREAKPOINT)
  protected readonly permissions = TRUST_PERMISSIONS
  protected readonly changing = signal<{
    id: string
    context: TrustContext
  } | null>(null)

  public readonly registry = input<string>()

  protected readonly filter = computed((url = this.registry()) =>
    url
      ? Object.values(this.trust.offers())
          .flat()
          .find(offer => sameUrl(offer.url, url)) || { url, name: url }
      : null,
  )

  protected readonly rows = computed((keys = this.trust.keys()) => {
    if (!keys) return undefined

    const offers = this.trust.offers()
    const filter = this.filter()

    return Object.entries(keys)
      .map(([id, key]) => ({
        id,
        key,
        type: keyType(key),
        identities: keyIdentities(key),
        offers: offers[id] || [],
      }))
      .filter(
        row =>
          !filter || row.offers.some(offer => sameUrl(offer.url, filter.url)),
      )
      .sort((a, b) => a.key.name.localeCompare(b.key.name))
  })

  protected async toggle(
    id: string,
    key: T.TrustedKey,
    context: TrustContext,
  ): Promise<void> {
    this.changing.set({ id, context })
    try {
      await this.trust.trust(id, key, context, !key[context])
    } finally {
      this.changing.set(null)
    }
  }

  protected async add(): Promise<void> {
    const res = await firstValueFrom(
      this.dialogs
        .openComponent<T.AddTrustedKeyRes | null>(ADD_TRUSTED_KEY)
        .pipe(defaultIfEmpty(null)),
    )

    if (res?.added.length) {
      await this.notify(
        'Key added',
        'These keys are now trusted for packages.',
        res.added,
      )
    }

    if (res?.existing.length) {
      await this.notify(
        'Key already present',
        'These keys were already present. Their trust is unchanged, and any new GPG identities were added.',
        res.existing,
      )
    }
  }

  private notify(
    label: i18nKey,
    message: i18nKey,
    ids: readonly string[],
  ): Promise<unknown> {
    return firstValueFrom(
      this.dialogs
        .openComponent<boolean>(TRUSTED_KEYS_NOTICE, {
          label,
          data: { message, ids, yes: 'Ok' },
        })
        .pipe(defaultIfEmpty(false)),
    )
  }
}
