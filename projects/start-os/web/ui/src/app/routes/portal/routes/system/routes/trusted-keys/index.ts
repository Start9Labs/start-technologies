import { KeyValue, KeyValuePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLoader,
} from '@taiga-ui/core'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { EDIT_TRUSTED_KEY } from 'src/app/components/trusted-keys/edit'
import { ADD_TRUSTED_KEY } from 'src/app/components/trusted-keys/form'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { TRUSTED_KEYRINGS_MENU_ITEM } from 'src/app/routes/portal/routes/system/system.const'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Trusted keyrings' | i18n }}
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Trusted keyrings' | i18n }}
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
        <button
          tuiButton
          size="xs"
          [style.margin]="'0 0.5rem 0 auto'"
          iconStart="@tui.plus"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </header>
      @if (trust.keys(); as keys) {
        <table
          [appTable]="[
            'Name',
            'Public keys',
            'Service Packages',
            'OS Updates',
            null,
          ]"
        >
          @for (entry of keys | keyvalue: compareKeyrings; track entry.key) {
            <tr>
              <td>
                <tui-icon [icon]="menuItem.icon" />
                {{ entry.value.name }}
              </td>
              <td [attr.data-label]="'Public keys' | i18n">
                {{ entry.value.publicKeys.length }}
              </td>
              <td [attr.data-label]="'Service Packages' | i18n">
                {{ (entry.value.marketplace ? 'Yes' : 'No') | i18n }}
              </td>
              <td [attr.data-label]="'OS Updates' | i18n">
                {{ (entry.value.os ? 'Yes' : 'No') | i18n }}
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
                  {{ 'More' | i18n }} {{ entry.value.name }}
                  <tui-data-list *tuiDropdown="let close" (click)="close()">
                    <button tuiOption (click)="edit(entry.key, entry.value)">
                      {{ 'Edit' | i18n }}
                    </button>
                    @if (entry.value.removable) {
                      <button
                        tuiOption
                        class="g-negative"
                        (click)="remove(entry.key)"
                      >
                        {{ 'Delete' | i18n }}
                      </button>
                    }
                  </tui-data-list>
                </button>
              </td>
            </tr>
          }
        </table>
      } @else {
        <tui-loader />
      }
    </section>
  `,
  styles: `
    td:first-child tui-icon {
      margin-inline-end: 0.5rem;
    }

    td:last-child {
      text-align: end;
    }

    :host-context(tui-root._mobile) {
      td[data-label] {
        grid-column: 1;

        &::before {
          content: attr(data-label) ': ';
          color: var(--tui-text-primary);
        }
      }

      td:last-child {
        grid-area: 1 / 2 / 5;
        align-content: center;
      }
    }
  `,
  imports: [
    KeyValuePipe,
    RouterLink,
    DocsLinkDirective,
    TableComponent,
    TitleDirective,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    TuiLoader,
    i18nPipe,
  ],
})
export default class TrustedKeyrings {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

  protected readonly menuItem = TRUSTED_KEYRINGS_MENU_ITEM
  protected readonly trust = inject(TrustedKeysService)
  protected readonly saving = signal(false)

  protected compareKeyrings(
    a: KeyValue<string, T.TrustedKeyring>,
    b: KeyValue<string, T.TrustedKeyring>,
  ): number {
    return (
      Number(a.value.removable) - Number(b.value.removable) ||
      (a.key < b.key ? -1 : a.key > b.key ? 1 : 0)
    )
  }

  protected add(): void {
    this.dialogs.openComponent(ADD_TRUSTED_KEY).subscribe()
  }

  protected edit(id: string, keyring: T.TrustedKeyring): void {
    this.dialogs
      .openComponent(EDIT_TRUSTED_KEY, { data: { id, keyring } })
      .subscribe()
  }

  protected async remove(id: string): Promise<void> {
    const confirmed = await firstValueFrom(
      this.dialogs
        .openConfirm({
          label: 'Delete trusted keyring?',
          data: {
            content:
              'Deleting this keyring removes it from both service-package and OS-update trust lists.',
            yes: 'Delete',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false)),
    )
    if (!confirmed) return
    this.saving.set(true)
    await this.tasks.run(async () => {
      await this.api.rpc.server.trustedKeys.remove({ id })
    }, 'Deleting')
    this.saving.set(false)
  }
}
