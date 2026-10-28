import { KeyValue, KeyValuePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { FormsModule, NgModel } from '@angular/forms'
import { TrustContext } from '@start9labs/marketplace'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiDialogContext,
  TuiIcon,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { ADD_TRUSTED_KEY } from './form'

@Component({
  template: `
    <header tuiHeader>
      <tui-icon icon="@tui.key-round" />
      <h2 tuiTitle [id]="context.id">{{ 'Manage trusted keyrings' | i18n }}</h2>
      <aside tuiAccessories>
        <button
          tuiButton
          iconStart="@tui.plus"
          [disabled]="saving()"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </aside>
    </header>
    <p>
      {{
        (context.data === 'marketplace'
          ? 'Trust these keyrings for service packages.'
          : 'Trust these keyrings for OS updates.'
        ) | i18n
      }}
    </p>
    @if (trust.keys(); as keys) {
      @for (entry of keys | keyvalue: compareKeyrings; track entry.key) {
        <div tuiCell>
          <span tuiTitle>{{ entry.value.name }}</span>
          <input
            #toggleControl="ngModel"
            tuiSwitch
            type="checkbox"
            [attr.aria-label]="entry.value.name"
            [ngModel]="entry.value[context.data]"
            [disabled]="saving()"
            (ngModelChange)="toggle(entry.key, $event, toggleControl)"
          />
          @if (entry.value.removable) {
            <button
              tuiIconButton
              appearance="flat-grayscale"
              iconStart="@tui.trash"
              [disabled]="saving()"
              (click)="remove(entry.key)"
            >
              {{ 'Delete' | i18n }} {{ entry.value.name }}
            </button>
          }
        </div>
      }
    } @else {
      <tui-loader />
    }
  `,
  imports: [
    FormsModule,
    KeyValuePipe,
    TuiButton,
    TuiCell,
    TuiHeader,
    TuiIcon,
    TuiLoader,
    TuiTitle,
    TuiSwitch,
    i18nPipe,
  ],
})
export class TrustedKeysDialog {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

  protected readonly context =
    injectContext<TuiDialogContext<void, TrustContext>>()
  protected readonly trust = inject(TrustedKeysService)
  protected readonly saving = signal(false)

  protected compareKeyrings(
    a: KeyValue<string, T.TrustedKey>,
    b: KeyValue<string, T.TrustedKey>,
  ): number {
    return (
      Number(a.value.removable) - Number(b.value.removable) ||
      (a.key < b.key ? -1 : a.key > b.key ? 1 : 0)
    )
  }

  protected add(): void {
    this.dialogs
      .openComponent(ADD_TRUSTED_KEY, { data: this.context.data })
      .subscribe()
  }

  protected async toggle(
    id: string,
    enabled: boolean,
    control: NgModel,
  ): Promise<void> {
    this.saving.set(true)
    const saved = await this.tasks.run(async () => {
      await this.api.rpc.server.trustedKeys.toggle({
        id,
        enabled,
        context: this.context.data,
      })
    }, 'Saving')
    if (!saved) {
      control.control.setValue(
        this.trust.keys()?.[id]?.[this.context.data] ?? false,
        { emitEvent: false, emitViewToModelChange: false },
      )
    }
    this.saving.set(false)
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

export const TRUSTED_KEYS = new PolymorpheusComponent(TrustedKeysDialog)
