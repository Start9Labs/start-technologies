import { Component, computed, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable } from '@taiga-ui/addon-table'
import { TuiButton, TuiDataList, TuiDropdown, TuiLink } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { Masked } from 'src/app/components/masked'
import { Placeholder } from 'src/app/components/placeholder'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { confirmPublishedPortDeletion } from 'src/app/services/published-port-deletion'
import { WifiService } from '../../service'
import {
  ADD_WIFI_PASSWORD,
  WifiPasswordDialogResult,
  WifiPasswordEntry,
} from './dialog'

@Component({
  template: `
    <table tuiTable class="g-table" [tuiSkeleton]="!service.data()">
      <thead>
        <tr>
          <th tuiTh [sorter]="'label' | tuiSorter">{{ 'Label' | i18n }}</th>
          <th tuiTh [sorter]="'password' | tuiSorter">
            {{ 'Password' | i18n }}
          </th>
          <th tuiTh [sorter]="'profile' | tuiSorter">
            {{ 'Security Profile' | i18n }}
          </th>
          <th tuiTh>
            <button tuiButton size="xs" iconStart="@tui.plus" (click)="add()">
              {{ 'Add' | i18n }}
            </button>
          </th>
        </tr>
      </thead>
      <tbody>
        @for (item of passwords() | tuiTableSort; track item.password) {
          <tr>
            <td tuiTd>{{ item.label }}</td>
            <td tuiTd>
              <span [appMasked]="item.password"></span>
            </td>
            <td tuiTd>
              <a tuiLink routerLink="/profiles">{{ item.profile }}</a>
            </td>
            <td tuiTd>
              <button
                tuiIconButton
                size="xs"
                iconStart="@tui.ellipsis-vertical"
                appearance="icon"
                tuiDropdownAlign="end"
                tuiDropdownAuto
                tuiDropdown
              >
                {{ 'Actions' | i18n }}
                <tui-data-list
                  *tuiDropdown="let close"
                  size="s"
                  (click)="close()"
                >
                  <button
                    tuiOption
                    iconStart="@tui.pencil"
                    (click)="edit(item)"
                  >
                    {{ 'Edit' | i18n }}
                  </button>
                  <button
                    tuiOption
                    class="g-negative"
                    iconStart="@tui.trash"
                    (click)="delete(item.password)"
                  >
                    {{ 'Delete' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td tuiTd colspan="4">
              <app-placeholder icon="@tui.wifi">
                {{ 'No Wi-Fi passwords configured' | i18n }}
              </app-placeholder>
            </td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    :host {
      max-width: 50rem;
    }

    td:first-child {
      font-weight: bold;
    }

    th:last-child,
    td:last-child {
      text-align: end;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    RouterLink,
    TuiButton,
    TuiTable,
    TuiLink,
    TuiDataList,
    TuiDropdown,
    TuiSkeleton,
    Masked,
    Placeholder,
    i18nPipe,
  ],
})
export default class WifiPasswords {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  // @TODO matt review
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  protected readonly service = inject(WifiService)
  protected readonly passwords = computed(
    () =>
      this.service.data()?.passwords.map(p => ({
        ...p,
        profile: p.profile?.fullname ?? 'Admin',
      })) || [],
  )

  // @TODO matt review
  async add() {
    const profiles = (await this.api.profilesList()).filter(
      p => p.interface !== 'lan',
    )
    this.dialogs
      .open<WifiPasswordDialogResult>(ADD_WIFI_PASSWORD, {
        label: this.i18n.transform('Add Wi-Fi Password'),
        data: { profiles },
      })
      .subscribe(result => {
        this.service.addPassword({
          label: result.label,
          profile: result.profile,
          password: result.password!,
        })
      })
  }

  // @TODO matt review
  async edit(entry: WifiPasswordEntry) {
    const profiles = (await this.api.profilesList()).filter(
      p => p.interface !== 'lan',
    )
    this.dialogs
      .open<WifiPasswordDialogResult>(ADD_WIFI_PASSWORD, {
        label: this.i18n.transform('Edit Wi-Fi Password'),
        data: { profiles, entry },
      })
      .subscribe(async result => {
        const current = this.service.data()
        if (!current) return
        const index = current.passwords.findIndex(
          p => p.password === entry.password,
        )
        if (index < 0) return
        const passwords = [...current.passwords]
        passwords[index] = {
          ...passwords[index],
          label: result.label,
          profile: result.profile,
        }
        const newConfig = { ...current, passwords }

        // A profile move strands its Wi-Fi devices' published ports.
        let pending
        try {
          pending = await this.service.previewWifi(newConfig)
        } catch {
          return
        }
        if (
          !(await confirmPublishedPortDeletion(
            this.dialogs,
            this.i18n,
            pending,
          ))
        )
          return

        await this.service.store(newConfig)
        this.service.refresh()
      })
  }

  async delete(password: string) {
    const current = this.service.data()
    if (!current) return
    const passwords = current.passwords.filter(p => p.password !== password)
    const newConfig = { ...current, passwords }

    // Removing a profile's last password strands its devices' published ports.
    let pending
    try {
      pending = await this.service.previewWifi(newConfig)
    } catch {
      return
    }
    if (!(await confirmPublishedPortDeletion(this.dialogs, this.i18n, pending)))
      return

    this.service.deletePassword(password)
  }
}
