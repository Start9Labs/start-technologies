import { Component, computed, inject, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { keyIdentities, keyType } from '@start9labs/marketplace'
import {
  DialogService,
  DocsLinkDirective,
  i18nKey,
  i18nPipe,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiChip } from '@taiga-ui/kit'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { ADD_TRUSTED_KEY } from 'src/app/components/trusted-keys/form'
import { TRUSTED_KEYS_NOTICE } from 'src/app/components/trusted-keys/notice'
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
          <th class="trust">{{ 'Packages' | i18n }}</th>
          <th class="trust">{{ 'StartOS updates' | i18n }}</th>
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
              <td class="trust" [attr.data-label]="'Packages' | i18n">
                <tui-icon
                  [icon]="row.key.marketplace ? '@tui.check' : '@tui.x'"
                  [class]="row.key.marketplace ? 'g-positive' : 'g-negative'"
                  [attr.aria-label]="
                    (row.key.marketplace ? 'Trusted' : 'Not trusted') | i18n
                  "
                />
              </td>
              <td class="trust" [attr.data-label]="'StartOS updates' | i18n">
                <tui-icon
                  [icon]="row.key.os ? '@tui.check' : '@tui.x'"
                  [class]="row.key.os ? 'g-positive' : 'g-negative'"
                  [attr.aria-label]="
                    (row.key.os ? 'Trusted' : 'Not trusted') | i18n
                  "
                />
              </td>
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
                >
                  {{ 'More' | i18n }} {{ row.key.name }}
                  <tui-data-list *tuiDropdown="let close" (click)="close()">
                    <button tuiOption (click)="trust.viewKey(row.id)">
                      {{ 'View details' | i18n }}
                    </button>
                    <button tuiOption (click)="trust.rename(row.id, row.key)">
                      {{ 'Edit label' | i18n }}
                    </button>
                    <button
                      tuiOption
                      (click)="
                        trust.trust(
                          row.id,
                          row.key,
                          'marketplace',
                          !row.key.marketplace
                        )
                      "
                    >
                      {{
                        (row.key.marketplace
                          ? 'Distrust for packages'
                          : 'Trust for packages'
                        ) | i18n
                      }}
                    </button>
                    <button
                      tuiOption
                      (click)="trust.trust(row.id, row.key, 'os', !row.key.os)"
                    >
                      {{
                        (row.key.os
                          ? 'Distrust for StartOS updates'
                          : 'Trust for StartOS updates'
                        ) | i18n
                      }}
                    </button>
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

    tui-icon {
      font-size: 1rem;
    }

    td:last-child {
      text-align: end;
    }

    :host-context(tui-root._mobile) {
      .trust {
        text-align: start;
      }

      .extra {
        display: none;
      }

      td[data-label] {
        grid-column: 1;

        &::before {
          content: attr(data-label) ': ';
          color: var(--tui-text-secondary);
        }
      }

      td:last-child {
        grid-area: 1 / 2 / 4;
        align-content: center;
      }
    }
  `,
  imports: [
    BackDirective,
    DocsLinkDirective,
    RouterLink,
    TableComponent,
    TitleDirective,
    TuiButton,
    TuiChip,
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    TuiLoader,
    TuiTitle,
    i18nPipe,
  ],
})
export default class TrustedKeys {
  private readonly dialogs = inject(DialogService)

  protected readonly trust = inject(TrustedKeysService)

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
