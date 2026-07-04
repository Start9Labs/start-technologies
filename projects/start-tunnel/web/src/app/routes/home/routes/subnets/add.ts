import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { ErrorService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiAutoFocus, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import { TuiButton, TuiDialogContext, TuiError, TuiInput } from '@taiga-ui/core'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiNotificationMiddleService,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import {
  matchWan,
  toWanItems,
  WanItem,
  wanLabel,
} from 'src/app/routes/home/components/wan'
import { ApiService } from 'src/app/services/api/api.service'

import { MappedDevice } from '../port-forwards/utils'

const CIDR_PATTERN =
  '^(?:(?:25[0-5]|2[0-4]\\d|1\\d{2}|[1-9]?\\d)\\.){3}(?:25[0-5]|2[0-4]\\d|1\\d{2}|[1-9]?\\d)/(?:[0-9]|1\\d|2[0-4])$'

// IPv4 (optional :port), bracketed IPv6 (optional :port), or bare IPv6. The
// backend validates strictly; this just guards against obvious typos.
const SERVER_PATTERN =
  '^(' +
  '(?:(?:25[0-5]|2[0-4]\\d|1\\d{2}|[1-9]?\\d)\\.){3}(?:25[0-5]|2[0-4]\\d|1\\d{2}|[1-9]?\\d)(?::\\d{1,5})?' +
  '|\\[[0-9a-fA-F:]+\\](?::\\d{1,5})?' +
  '|[0-9a-fA-F:]+' +
  ')$'

// A bare IPv6 prefix in CIDR form (addr/len 0-128; /64 is typical). Empty clears
// it. The backend validates strictly and checks the server can actually route it.
const IPV6_CIDR_PATTERN = '^[0-9a-fA-F:]+/(?:12[0-8]|1[01]\\d|[1-9]?\\d)$'

const MODE_LABEL: Record<T.Tunnel.DnsMode, string> = {
  default: 'Default (VPS provider)',
  device: 'Device',
  custom: 'Custom',
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>Name</label>
        <input tuiInput tuiAutoFocus formControlName="name" />
      </tui-textfield>
      <tui-error formControlName="name" />

      @if (!context.data.name) {
        <tui-textfield>
          <label tuiLabel>IP Range</label>
          <input tuiInput formControlName="subnet" />
        </tui-textfield>
        <tui-error formControlName="subnet" />
      }

      <tui-textfield
        tuiChevron
        [tuiTextfieldCleaner]="false"
        [stringify]="modeLabel"
      >
        <label tuiLabel>DNS</label>
        @if (mobile) {
          <select tuiSelect formControlName="mode" [items]="modes"></select>
        } @else {
          <input tuiSelect formControlName="mode" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="modes" />
        }
      </tui-textfield>

      @switch (mode()) {
        @case ('device') {
          @if (context.data.devices.length) {
            <tui-textfield
              tuiChevron
              [tuiTextfieldCleaner]="false"
              [stringify]="stringifyDevice"
            >
              <label tuiLabel>Device</label>
              @if (mobile) {
                <select
                  tuiSelect
                  formControlName="device"
                  placeholder="Select device"
                  [items]="context.data.devices"
                ></select>
              } @else {
                <input tuiSelect formControlName="device" />
              }
              @if (!mobile) {
                <tui-data-list-wrapper
                  *tuiDropdown
                  [items]="context.data.devices"
                />
              }
            </tui-textfield>
          } @else {
            <p>Add a device to this subnet first.</p>
          }
        }
        @case ('custom') {
          @for (control of servers.controls; track $index) {
            <tui-textfield>
              <label tuiLabel>Server {{ $index + 1 }}</label>
              <input tuiInput [formControl]="control" placeholder="1.1.1.1" />
              @if (servers.length > 1) {
                <button
                  tuiIconButton
                  type="button"
                  appearance="flat-grayscale"
                  iconStart="@tui.x"
                  (click)="removeServer($index)"
                >
                  Remove
                </button>
              }
            </tui-textfield>
          }
          @if (servers.length < 3) {
            <button
              tuiButton
              type="button"
              size="s"
              appearance="flat"
              iconStart="@tui.plus"
              (click)="addServer()"
            >
              Add server
            </button>
          }
        }
      }

      <tui-textfield
        tuiChevron
        [identityMatcher]="matchWan"
        [tuiTextfieldCleaner]="false"
        [stringify]="stringifyWan"
      >
        <label tuiLabel>WAN IP</label>
        @if (mobile) {
          <select tuiSelect formControlName="wanIp" [items]="wanItems"></select>
        } @else {
          <input tuiSelect formControlName="wanIp" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="wanItems" />
        }
      </tui-textfield>

      <tui-textfield>
        <label tuiLabel>IPv6 Prefix</label>
        <input
          tuiInput
          formControlName="ipv6"
          placeholder="2001:db8:abcd::/64"
        />
      </tui-textfield>
      <tui-error formControlName="ipv6" />

      <footer>
        <button tuiButton type="button" (click)="onSave()">Save</button>
      </footer>
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiAutoFocus,
    TuiButton,
    TuiChevron,
    TuiDataListWrapper,
    TuiError,
    TuiForm,
    TuiInput,
    TuiSelect,
  ],
})
export class SubnetsAdd {
  private readonly api = inject(ApiService)
  private readonly loading = inject(TuiNotificationMiddleService)
  private readonly errorService = inject(ErrorService)
  private readonly fb = inject(NonNullableFormBuilder)

  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly context = injectContext<TuiDialogContext<void, Data>>()
  protected readonly modes: readonly T.Tunnel.DnsMode[] = [
    'default',
    'device',
    'custom',
  ]

  protected readonly form = this.fb.group({
    name: this.fb.control(this.context.data.name ?? '', Validators.required),
    subnet: this.fb.control(this.context.data.subnet, [
      Validators.required,
      Validators.pattern(CIDR_PATTERN),
    ]),
    mode: this.fb.control<T.Tunnel.DnsMode>(this.context.data.mode),
    device: this.fb.control<MappedDevice | null>(this.context.data.device),
    servers: this.fb.array(
      (this.context.data.servers.length ? this.context.data.servers : ['']).map(
        s => this.serverControl(s),
      ),
    ),
    wanIp: this.fb.control<WanItem>({ ip: this.context.data.wanIp }),
    ipv6: this.fb.control(
      this.context.data.ipv6 ?? '',
      Validators.pattern(IPV6_CIDR_PATTERN),
    ),
  })

  protected readonly mode = toSignal(this.form.controls.mode.valueChanges, {
    initialValue: this.form.controls.mode.value,
  })

  protected readonly wanItems = toWanItems(this.context.data.wanOptions)

  protected get servers() {
    return this.form.controls.servers
  }

  protected readonly modeLabel = (m: T.Tunnel.DnsMode) => MODE_LABEL[m]
  protected readonly matchWan = matchWan
  protected readonly stringifyWan = ({ ip }: WanItem) =>
    wanLabel(ip, 'System default', this.context.data.defaultWan)
  protected readonly stringifyDevice = ({ ip, name }: MappedDevice) =>
    ip ? `${name} (${ip})` : ''

  protected addServer(): void {
    if (this.servers.length < 3) this.servers.push(this.serverControl(''))
  }

  protected removeServer(index: number): void {
    this.servers.removeAt(index)
  }

  protected async onSave(): Promise<void> {
    const editing = !!this.context.data.name

    if (this.form.controls.name.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form.controls.name)
      return
    }
    if (!editing && this.form.controls.subnet.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form.controls.subnet)
      return
    }
    const mode = this.form.controls.mode.value
    if (mode === 'device' && !this.form.controls.device.value) {
      return
    }
    if (mode === 'custom' && (this.servers.invalid || !this.servers.length)) {
      tuiMarkControlAsTouchedAndValidate(this.servers)
      return
    }
    if (this.form.controls.ipv6.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form.controls.ipv6)
      return
    }

    const loader = this.loading.open('').subscribe()
    const { name, subnet } = this.form.getRawValue()

    try {
      editing
        ? await this.api.editSubnet({ subnet, name })
        : await this.api.addSubnet({ subnet, name })

      if (this.dnsChanged()) {
        await this.api.setSubnetDns({
          subnet,
          mode,
          deviceIp:
            mode === 'device'
              ? (this.form.controls.device.value?.ip ?? null)
              : null,
          servers: mode === 'custom' ? this.servers.getRawValue() : [],
        })
      }

      const wanIp = this.form.controls.wanIp.value.ip
      if (wanIp !== this.context.data.wanIp) {
        await this.api.setSubnetWan({ subnet, wanIp })
      }

      const ipv6 = this.form.controls.ipv6.value.trim() || null
      if (ipv6 !== (this.context.data.ipv6 ?? null)) {
        await this.api.setSubnetIpv6({ subnet, prefix: ipv6 })
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      loader.unsubscribe()
      this.context.$implicit.complete()
    }
  }

  // Avoid an unnecessary DNS proxy resync (which briefly rebinds every subnet's
  // listener) when only the name changed.
  private dnsChanged(): boolean {
    const mode = this.form.controls.mode.value
    if (mode !== this.context.data.mode) return true
    if (mode === 'device') {
      return (
        (this.form.controls.device.value?.ip ?? null) !==
        (this.context.data.device?.ip ?? null)
      )
    }
    if (mode === 'custom') {
      const next = this.servers.getRawValue()
      const prev = this.context.data.servers
      return next.length !== prev.length || next.some((s, i) => s !== prev[i])
    }
    return false
  }

  private serverControl(value: string) {
    return this.fb.control(value, [
      Validators.required,
      Validators.pattern(SERVER_PATTERN),
    ])
  }
}

export const SUBNETS_ADD = new PolymorpheusComponent(SubnetsAdd)

interface Data {
  name?: string
  subnet: string
  mode: T.Tunnel.DnsMode
  device: MappedDevice | null
  servers: readonly string[]
  devices: readonly MappedDevice[]
  wanIp: string | null
  wanOptions: readonly string[]
  defaultWan: string | null
  ipv6: string | null
}
