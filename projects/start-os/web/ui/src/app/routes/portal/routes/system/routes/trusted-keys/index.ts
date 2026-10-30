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
import { TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiLoader,
  TuiScrollbar,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { EDIT_TRUSTED_KEY } from 'src/app/components/trusted-keys/edit'
import { ADD_TRUSTED_KEY } from 'src/app/components/trusted-keys/form'
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
      <header tuiHeader>
        <h2 tuiTitle>{{ 'Trusted keyrings' | i18n }}</h2>
        <a
          tuiIconButton
          docsLink
          path="/start-os/trusted-keys.html"
          size="s"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="add()">
            {{ 'Add' | i18n }}
          </button>
        </aside>
      </header>
      @if (trust.keys(); as keys) {
        <tui-scrollbar>
          <table tuiTable size="s">
            <thead>
              <tr>
                <th tuiTh>{{ 'Name' | i18n }}</th>
                <th tuiTh>{{ 'Public keys' | i18n }}</th>
                <th tuiTh>{{ 'Packages' | i18n }}</th>
                <th tuiTh>{{ 'OS' | i18n }}</th>
                <th tuiTh>{{ 'Actions' | i18n }}</th>
              </tr>
            </thead>
            <tbody>
              @for (
                entry of keys | keyvalue: compareKeyrings;
                track entry.key
              ) {
                <tr>
                  <td tuiTd>{{ entry.value.name }}</td>
                  <td tuiTd>{{ entry.value.publicKeys.length }}</td>
                  <td tuiTd>
                    {{
                      (entry.value.marketplace ? 'Enabled' : 'Disabled') | i18n
                    }}
                  </td>
                  <td tuiTd>
                    {{ (entry.value.os ? 'Enabled' : 'Disabled') | i18n }}
                  </td>
                  <td tuiTd>
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
                        <button
                          tuiOption
                          (click)="edit(entry.key, entry.value)"
                        >
                          {{ 'Edit' | i18n }}
                        </button>
                        @if (entry.value.removable) {
                          <button tuiOption (click)="remove(entry.key)">
                            {{ 'Delete' | i18n }}
                          </button>
                        }
                      </tui-data-list>
                    </button>
                  </td>
                </tr>
              }
            </tbody>
          </table>
        </tui-scrollbar>
      } @else {
        <tui-loader />
      }
    </section>
  `,
  styles: `
    table {
      inline-size: 100%;
    }
  `,
  imports: [
    KeyValuePipe,
    RouterLink,
    DocsLinkDirective,
    TitleDirective,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiHeader,
    TuiLoader,
    TuiScrollbar,
    TuiTable,
    TuiTitle,
    i18nPipe,
  ],
})
export default class TrustedKeyrings {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

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
