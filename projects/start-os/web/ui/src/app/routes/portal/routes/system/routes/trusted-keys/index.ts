import { KeyValue, KeyValuePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { FormsModule, NgModel } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { KeyIdentityPipe } from '@start9labs/marketplace'
import {
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDataList, TuiDropdown, TuiLoader } from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { EDIT_TRUSTED_KEY } from 'src/app/components/trusted-keys/edit'
import { ADD_TRUSTED_KEY } from 'src/app/components/trusted-keys/form'
import { KeyIdentity } from 'src/app/components/trusted-keys/identity'
import { RegistryOrigin } from 'src/app/components/trusted-keys/origin'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
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
        {{ 'Manage Trust' | i18n }}
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Manage Trust' | i18n }}
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
      <p>
        {{
          'System keys and keys listed by a connected registry can only be disabled.'
            | i18n
        }}
      </p>
      @if (trust.keys(); as keys) {
        <table [appTable]="['Name', 'Origin', 'Packages', 'OS Updates', null]">
          @for (entry of keys | keyvalue: compareKeyrings; track entry.key) {
            @let identity = entry.value | keyIdentity;
            <tr>
              <td><span [appKeyIdentity]="entry.value"></span></td>
              <td [attr.data-label]="'Origin' | i18n">
                @switch (entry.value.origin) {
                  @case ('system') {
                    {{ 'System' | i18n }}
                  }
                  @case ('manual') {
                    {{ 'Manually Added' | i18n }}
                  }
                  @default {
                    <div
                      [appRegistryOrigin]="entry.value.origin"
                      [identity]="entry.value.originRegistry"
                    ></div>
                  }
                }
              </td>
              <td [attr.data-label]="'Packages' | i18n">
                <input
                  #packages="ngModel"
                  tuiSwitch
                  type="checkbox"
                  [attr.aria-label]="
                    ('Packages' | i18n) + ' — ' + identity.name
                  "
                  [ngModel]="entry.value.marketplace"
                  [disabled]="saving()"
                  (ngModelChange)="
                    update(
                      entry.key,
                      entry.value,
                      'marketplace',
                      $event,
                      packages
                    )
                  "
                />
              </td>
              <td [attr.data-label]="'OS Updates' | i18n">
                <input
                  #os="ngModel"
                  tuiSwitch
                  type="checkbox"
                  [attr.aria-label]="
                    ('OS Updates' | i18n) + ' — ' + identity.name
                  "
                  [ngModel]="entry.value.os"
                  [disabled]="saving()"
                  (ngModelChange)="
                    update(entry.key, entry.value, 'os', $event, os)
                  "
                />
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
                  {{ 'More' | i18n }} {{ identity.name }}
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
    header > button {
      margin-inline-start: auto;
    }
    section > p {
      margin: 1rem;
      color: var(--tui-text-secondary);
    }
    td:first-child {
      max-inline-size: 26rem;
    }
    td:nth-child(2) {
      overflow-wrap: anywhere;
    }
    td:last-child {
      text-align: end;
    }
    :host-context(tui-root._mobile) {
      td[data-label] {
        grid-column: 1;
        &::before {
          content: attr(data-label) ': ';
          color: var(--tui-text-secondary);
        }
      }
      td:last-child {
        grid-area: 1 / 2 / 5;
        align-content: center;
      }
    }
  `,
  imports: [
    FormsModule,
    KeyIdentity,
    KeyIdentityPipe,
    RegistryOrigin,
    KeyValuePipe,
    RouterLink,
    DocsLinkDirective,
    TableComponent,
    TitleDirective,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiLoader,
    TuiSwitch,
    i18nPipe,
  ],
})
export default class TrustedKeys {
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

  protected async update(
    id: string,
    key: T.TrustedKeyring,
    context: 'marketplace' | 'os',
    enabled: boolean,
    control: NgModel,
  ): Promise<void> {
    this.saving.set(true)
    const saved = await this.tasks.run(
      () =>
        this.api.rpc.server.trustedKeys.update({
          id,
          name: key.name,
          marketplace: key.marketplace,
          os: key.os,
          [context]: enabled,
        }),
      'Saving',
    )
    if (!saved) control.reset(key[context])
    this.saving.set(false)
  }

  protected async remove(id: string): Promise<void> {
    const confirmed = await firstValueFrom(
      this.dialogs
        .openConfirm({
          label: 'Delete key?',
          data: {
            content:
              'Deleting this key removes it from both package and OS-update trust.',
            yes: 'Delete',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false)),
    )
    if (!confirmed) return
    this.saving.set(true)
    await this.tasks.run(
      () => this.api.rpc.server.trustedKeys.remove({ id }),
      'Deleting',
    )
    this.saving.set(false)
  }
}
