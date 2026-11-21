import { DatePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import { WA_WINDOW } from '@ng-web-apis/common'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { auth, ISB, T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { filter } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'

type PasskeyRow = T.Passkey & { readonly publicKey: string }

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Passkeys' | i18n }}
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Passkeys' | i18n }}
        <button
          tuiButton
          size="xs"
          iconStart="@tui.plus"
          [style.margin-inline-start]="'auto'"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </header>
      <table [appTable]="['Name', 'Added', null]">
        @for (passkey of passkeys(); track passkey.publicKey) {
          <tr>
            <td>{{ passkey.name }}</td>
            <td>{{ passkey.addedAt | date: 'medium' }}</td>
            <td>
              <button
                tuiIconButton
                size="xs"
                appearance="flat-destructive"
                iconStart="@tui.trash"
                (click)="remove(passkey)"
              >
                {{ 'Remove' | i18n }}
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td colspan="3">
              @if (passkeys()) {
                <app-placeholder icon="@tui.fingerprint">
                  {{ 'No passkeys' | i18n }}
                </app-placeholder>
              } @else {
                <div [tuiSkeleton]="true">{{ 'Loading' | i18n }}</div>
              }
            </td>
          </tr>
        }
      </table>
    </section>
  `,
  styles: `
    :host {
      max-width: 50rem;
    }
  `,
  imports: [
    DatePipe,
    RouterLink,
    TuiButton,
    TuiSkeleton,
    PlaceholderComponent,
    TableComponent,
    TitleDirective,
    i18nPipe,
  ],
})
export default class Passkeys {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly win = inject(WA_WINDOW)

  protected readonly passkeys = signal<readonly PasskeyRow[] | null>(null)

  constructor() {
    this.tasks.runInline(() => this.load())
  }

  async add() {
    const spec = ISB.InputSpec.of({
      name: ISB.Value.text({
        name: this.i18n.transform('Name'),
        required: true,
        default: null,
      }),
      password: ISB.Value.text({
        name: this.i18n.transform('Password'),
        required: true,
        default: null,
        masked: true,
      }),
    })

    this.formDialog.open(FormComponent, {
      label: 'Add passkey',
      data: {
        spec: await configBuilderToSpec(spec),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: async ({ name, password }: typeof spec._TYPE) =>
              this.tasks.run(async () => {
                const passkey = await auth.createPasskey(
                  {
                    id: this.win.crypto.getRandomValues(new Uint8Array(16)),
                    name: this.win.location.hostname,
                    displayName: name,
                  },
                  'StartOS',
                )
                await this.api.rpc.auth.passkey.add({
                  name,
                  credentialId: passkey.credentialId,
                  publicKey: passkey.pubkeyPem,
                  password,
                })
                await this.load()
              }, 'Saving'),
          },
        ],
      },
    })
  }

  remove({ publicKey }: PasskeyRow) {
    this.dialogs
      .openConfirm({ label: 'Are you sure?', size: 's' })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          await this.api.rpc.auth.passkey.remove({ publicKey })
          await this.load()
        }, 'Removing'),
      )
  }

  private async load() {
    const passkeys = await this.api.rpc.auth.passkey.list({})
    this.passkeys.set(
      Object.entries(passkeys).map(([publicKey, passkey]) => ({
        publicKey,
        ...passkey,
      })),
    )
  }
}
