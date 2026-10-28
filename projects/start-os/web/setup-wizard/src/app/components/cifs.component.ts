import { CommonModule } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import {
  FormControl,
  FormGroup,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { DialogService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiPassword } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from '../services/api.service'
import { StartOSDiskInfoWithId } from '../types'
import { normalizeCifsPath } from '../utils/cifs'

export interface CifsResult {
  cifs: T.Cifs
  servers: StartOSDiskInfoWithId[]
}

@Component({
  template: `
    <form [formGroup]="form" (ngSubmit)="submit()">
      <tui-textfield>
        <label tuiLabel>{{ 'Hostname' | i18n }}*</label>
        <input
          tuiInput
          formControlName="hostname"
          placeholder="e.g. 'My Computer' OR 'my-computer.local'"
        />
      </tui-textfield>
      <tui-error formControlName="hostname" [order]="['required']" />

      <tui-textfield class="input">
        <label tuiLabel>{{ 'Path' | i18n }}*</label>
        <input
          tuiInput
          formControlName="path"
          placeholder="/Desktop/my-folder"
        />
      </tui-textfield>
      <tui-error formControlName="path" />

      <tui-textfield class="input">
        <label tuiLabel>{{ 'Username' | i18n }}*</label>
        <input
          tuiInput
          formControlName="username"
          placeholder="Enter username"
        />
      </tui-textfield>
      <tui-error formControlName="username" />

      <tui-textfield class="input">
        <label tuiLabel>{{ 'Password' | i18n }}</label>
        <input tuiInput type="password" formControlName="password" />
        <tui-icon tuiPassword />
      </tui-textfield>

      <footer>
        <button
          tuiButton
          appearance="secondary"
          type="button"
          [disabled]="connecting()"
          (click)="cancel()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [disabled]="form.invalid" [loading]="connecting()">
          {{ 'Connect' | i18n }}
        </button>
      </footer>
    </form>
  `,
  styles: `
    .input {
      margin-top: 1rem;
    }

    footer {
      display: flex;
      gap: 1rem;
      margin-top: 1.5rem;
    }
  `,
  imports: [
    CommonModule,
    ReactiveFormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiInput,
    TuiPassword,
    TuiError,
    TuiIcon,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider({
      required: 'This field is required',
    }),
  ],
})
export class CifsComponent {
  private readonly dialogs = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly context = injectContext<TuiDialogContext<CifsResult>>()

  readonly connecting = signal(false)

  readonly form = new FormGroup({
    hostname: new FormControl('', {
      validators: [
        Validators.required,
        Validators.pattern(/^[a-zA-Z0-9._-]+( [a-zA-Z0-9]+)*$/),
      ],
      nonNullable: true,
    }),
    path: new FormControl('', {
      validators: [Validators.required],
      nonNullable: true,
    }),
    username: new FormControl('', {
      validators: [Validators.required],
      nonNullable: true,
    }),
    password: new FormControl(),
  })

  cancel() {
    this.context.$implicit.complete()
  }

  async submit(): Promise<void> {
    this.connecting.set(true)

    try {
      const diskInfo = await this.api.rpc.setup.cifs.verify({
        ...this.form.getRawValue(),
        path: normalizeCifsPath(this.form.getRawValue().path),
        password: this.form.value.password
          ? await this.api.encrypt(String(this.form.value.password))
          : null,
      })

      const servers = Object.keys(diskInfo).map(id => ({
        id,
        ...diskInfo[id]!,
      }))

      this.context.completeWith({
        cifs: { ...this.form.getRawValue() },
        servers,
      })
    } catch (e) {
      this.connecting.set(false)
      this.onFail()
    }
  }

  private onFail() {
    this.dialogs
      .openAlert(
        'Unable to connect to network folder. Ensure (1) target computer is connected to LAN, (2) target folder is being shared, and (3) hostname, path, and credentials are accurate.',
        { label: 'Connection Failed' },
      )
      .subscribe()
  }
}

export const CIFS = new PolymorpheusComponent(CifsComponent)
