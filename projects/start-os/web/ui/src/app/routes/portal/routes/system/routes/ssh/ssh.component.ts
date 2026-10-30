import { CommonModule } from '@angular/common'
import { Component, inject, viewChild } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB, T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { filter, from, merge, Subject } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { SSHTableComponent } from './table.component'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'SSH Access' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/ssh.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>
    @let keys = keys$ | async;
    <section class="g-card">
      <header>
        {{ 'SSH Access' | i18n }}
        @if (tableKeys()?.selected()?.length) {
          <button
            tuiButton
            class="delete"
            size="xs"
            appearance="primary-destructive"
            (click)="remove(keys || [])"
          >
            {{ 'Delete' | i18n }}
          </button>
        } @else {
          <button
            tuiButton
            size="xs"
            iconStart="@tui.plus"
            [style.margin]="'0 0.5rem 0 auto'"
            (click)="add(keys || [])"
          >
            {{ 'Add' | i18n }}
          </button>
        }
      </header>
      <div #table [keys]="keys" (deleteSelected)="remove(keys || [])"></div>
    </section>
  `,
  styles: `
    :host {
      max-width: 70rem;
    }

    :host-context(tui-root:not(._mobile)) .delete {
      display: none;
    }
  `,
  imports: [
    CommonModule,
    TuiButton,
    SSHTableComponent,
    RouterLink,
    TitleDirective,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class SystemSSHComponent {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly formDialog = inject(FormDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly dialogs = inject(DialogService)

  private readonly local$ = new Subject<readonly T.SshKeyResponse[]>()

  readonly keys$ = merge(from(this.api.rpc.ssh.list({})), this.local$)

  protected tableKeys = viewChild<SSHTableComponent<T.SshKeyResponse>>('table')

  async add(all: readonly T.SshKeyResponse[]) {
    const spec = ISB.InputSpec.of({
      key: ISB.Value.text({
        name: this.i18n.transform('Public Key'),
        required: true,
        default: null,
        patterns: [
          {
            regex:
              '^(ssh-(rsa|ed25519|dss|ecdsa)|ecdsa-sha2-nistp(256|384|521))\\s+[A-Za-z0-9+/=]+(\\s[^\\s]+)?$',
            description: this.i18n.transform('must be a valid SSH public key'),
          },
        ],
      }),
    })

    this.formDialog.open(FormComponent, {
      label: 'Add SSH key',
      data: {
        spec: await configBuilderToSpec(spec),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: async ({ key }: typeof spec._TYPE) =>
              this.tasks.run(async () => {
                const newKey = await this.api.rpc.ssh.add({ key })
                this.local$.next([newKey, ...all])
              }, 'Saving'),
          },
        ],
      },
    })
  }

  remove(all: readonly T.SshKeyResponse[]) {
    this.dialogs
      .openConfirm({ label: 'Are you sure?', size: 's' })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        const selected = this.tableKeys()?.selected() || []
        const fingerprints = selected.map(s => s.fingerprint) || []

        this.tasks.run(async () => {
          await Promise.all(
            fingerprints.map(fingerprint =>
              this.api.rpc.ssh.remove({ fingerprint }),
            ),
          )
          this.local$.next(
            all.filter(s => !fingerprints.includes(s.fingerprint)),
          )
          this.tableKeys()?.selected.set([])
        }, 'Deleting')
      })
  }
}
