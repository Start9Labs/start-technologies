import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'

export type WanIpDialogData = {
  detected: string | null
  override: string | null
}

// Anything outside globally routable IPv4 space cannot be where the internet
// reaches this server. Mirrors the backend's `check_wan_ip_override`.
const RESERVED: ReadonlyArray<readonly [string, number]> = [
  ['0.0.0.0', 8],
  ['10.0.0.0', 8],
  ['100.64.0.0', 10],
  ['127.0.0.0', 8],
  ['169.254.0.0', 16],
  ['172.16.0.0', 12],
  ['192.0.0.0', 24],
  ['192.0.2.0', 24],
  ['192.168.0.0', 16],
  ['198.18.0.0', 15],
  ['198.51.100.0', 24],
  ['203.0.113.0', 24],
  ['224.0.0.0', 3],
]

function toU32(value: string): number | null {
  const octets = value.split('.')

  if (octets.length !== 4) return null

  return octets.reduce<number | null>((acc, octet) => {
    const parsed = Number(octet)

    return acc === null || !/^\d{1,3}$/.test(octet) || parsed > 255
      ? null
      : acc * 256 + parsed
  }, 0)
}

export function isPublicIpv4(value: string): boolean {
  const address = toU32(value)

  if (address === null) return false

  return !RESERVED.some(([block, bits]) => {
    const base = toU32(block)
    const mask = bits === 0 ? 0 : (-1 << (32 - bits)) >>> 0

    return base !== null && ((address ^ base) & mask) === 0
  })
}

const publicIpv4: ValidatorFn = ({ value }) =>
  isPublicIpv4(String(value).trim()) ? null : { publicIpv4: true }

@Component({
  template: `
    <form [formGroup]="form" (submit.prevent)="save()">
      <p class="explainer">
        {{
          'StartOS reads this from outbound traffic. Set it by hand when inbound traffic arrives on a different address, such as when your router sends outbound traffic through a VPN.'
            | i18n
        }}
      </p>
      <tui-textfield>
        <label tuiLabel>{{ 'WAN IP' | i18n }}</label>
        <input tuiInput inputmode="decimal" formControlName="ip" />
      </tui-textfield>
      <tui-error formControlName="ip" />
      <p class="detected">
        {{ 'Detected by StartOS:' | i18n }}
        {{ context.data.detected || ('No WAN IP' | i18n) }}
      </p>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          [disabled]="!context.data.override"
          (click)="context.completeWith(null)"
        >
          {{ 'Reset to detected' | i18n }}
        </button>
        <button tuiButton [disabled]="form.invalid">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  styles: `
    .explainer,
    .detected {
      color: var(--tui-text-secondary);
      font: var(--tui-typography-body-s);
    }

    .explainer {
      margin: 0 0 1rem;
    }

    .detected {
      margin: 0.25rem 0 0;
    }

    footer {
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
      margin-top: 1.5rem;
    }
  `,
  imports: [ReactiveFormsModule, TuiButton, TuiError, TuiInput, i18nPipe],
  providers: [
    tuiValidationErrorsProvider(() => ({
      required: inject(i18nPipe).transform('Required'),
      publicIpv4: inject(i18nPipe).transform('Must be a public IPv4 address'),
    })),
  ],
})
export class WanIpDialog {
  protected readonly context =
    injectContext<TuiDialogContext<string | null, WanIpDialogData>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    ip: [
      this.context.data.override ?? this.context.data.detected ?? '',
      [Validators.required, publicIpv4],
    ],
  })

  protected save() {
    this.context.completeWith(this.form.controls.ip.value.trim())
  }
}
