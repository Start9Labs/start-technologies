import { Component, computed, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  FormsModule,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiAnimated,
  TuiContext,
  tuiMarkControlAsTouchedAndValidate,
  TuiStringHandler,
} from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDataList,
  TuiDialogContext,
  TuiError,
  TuiGroup,
  TuiInput,
  TuiLabel,
  TuiSelectLike,
  TuiTextfield,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiInputChip,
  TuiInputNumber,
  TuiMultiSelect,
  TuiRadioList,
  TuiSegmented,
  TuiSelect,
  TuiStringifyPipe,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiElasticContainer, TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ScheduleComponent } from 'src/app/components/schedule'
import { provideHelp } from 'src/app/help/help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ModalHelp } from 'src/app/help/modal-help'
import { PROFILE_SCHEDULE } from 'src/app/routes/profiles/schedule'
import type {
  DnsServer,
  LanAccess,
  ProfileIdOpt,
  ScheduleWindow,
  SecurityProfile,
  WanAccess,
} from 'src/app/services/api/api.service'
import { CustomValidators } from 'src/app/utils/validators'

export interface ProfileDialogData {
  existing?: SecurityProfile
  otherProfiles: SecurityProfile[]
  outboundVpns: Array<{ interface: string; label: string }>
  usedSubnets: number[]
  subnetBase: { firstOctet: number; secondOctet: number }
  hasStaticIpsInSubnet?: boolean
  scheduleWindows?: ScheduleWindow[]
}

export interface ProfileDialogResult {
  fullname?: string
  gateway_ip: string
  outbound: string
  lan_access: LanAccess<ProfileIdOpt>
  wan_access: WanAccess
  access_to_new_profiles: boolean
  owns_lan: boolean
  dns_override?: DnsServer[]
  schedule_windows: ScheduleWindow[]
}

// At least one checkbox in the group must be ticked.
function atLeastOneSelected(group: AbstractControl): ValidationErrors | null {
  return Object.values(group.value ?? {}).some(Boolean)
    ? null
    : { required: true }
}

// The comma-separated list must contain at least one non-blank entry.
function nonEmptyList(control: AbstractControl): ValidationErrors | null {
  return String(control.value ?? '')
    .split(',')
    .some(entry => entry.trim())
    ? null
    : { required: true }
}

@Component({
  template: `
    <form
      tuiForm="m"
      class="g-form"
      [formGroup]="form"
      (submit.prevent)="save()"
    >
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input
          tuiInput
          formControlName="fullname"
          [placeholder]="'e.g. Guest' | i18n"
        />
      </tui-textfield>
      <tui-error formControlName="fullname" />

      <tui-segmented [(activeItemIndex)]="tab">
        <button type="button">{{ 'LAN' | i18n }}</button>
        <button type="button">{{ 'WAN / Internet' | i18n }}</button>
        <button type="button">{{ 'DNS' | i18n }}</button>
      </tui-segmented>

      @switch (tab()) {
        @case (0) {
          <fieldset>
            <legend>{{ 'Subnet' | i18n }}</legend>
            <div tuiGroup>
              <tui-textfield>
                <input tuiInput [value]="subnetBase().firstOctet" disabled />
              </tui-textfield>
              <tui-textfield>
                <input tuiInput [value]="subnetBase().secondOctet" disabled />
              </tui-textfield>
              <tui-textfield>
                <input
                  tuiInputNumber
                  formControlName="subnet"
                  [min]="0"
                  [max]="254"
                />
              </tui-textfield>
              <tui-textfield>
                <input tuiInput value="1" disabled />
              </tui-textfield>
            </div>
          </fieldset>
          <tui-error formControlName="subnet" />
          <fieldset>
            <legend>{{ 'Access' | i18n }}</legend>
            <tui-radio-list
              size="s"
              formControlName="lanAccessType"
              [items]="lanAccessOptions"
              [itemContent]="getLanAccessLabel"
              [style.flex-direction]="'row'"
            />
          </fieldset>
          <tui-elastic-container>
            @if (['whitelist', 'blacklist'].includes(lanAccessType())) {
              <tui-textfield
                multi
                tuiAnimated
                tuiChevron
                [stringify]="'fullname' | tuiStringify"
              >
                @if (!context.data.otherProfiles.length) {
                  <label tuiLabel>{{ 'No other profiles' | i18n }}</label>
                }
                <input
                  tuiInputChip
                  tuiSelectLike
                  [disabled]="!context.data.otherProfiles.length"
                  [invalid]="
                    form.controls.lanAccessProfiles.touched &&
                    form.controls.lanAccessProfiles.invalid
                  "
                  [placeholder]="
                    multi().length ? '' : ('Allowed profiles' | i18n)
                  "
                  [ngModelOptions]="{ standalone: true }"
                  [(ngModel)]="multi"
                  (ngModelChange)="onWhitelist($event)"
                />
                <tui-input-chip *tuiItem />
                <tui-data-list-wrapper
                  *tuiDropdown
                  tuiMultiSelectGroup
                  [items]="context.data.otherProfiles"
                />
              </tui-textfield>
              <tui-error [formGroup]="form.controls.lanAccessProfiles" />

              <label tuiLabel>
                <input
                  type="checkbox"
                  tuiCheckbox
                  formControlName="accessToNewProfiles"
                />
                {{ 'Auto whitelist new profiles' | i18n }}
              </label>
            }
          </tui-elastic-container>
        }
        @case (1) {
          <fieldset class="section">
            <legend>{{ 'Outbound Routing' | i18n }}</legend>
            <tui-radio-list
              size="s"
              formControlName="outboundType"
              [items]="outboundTypeOptions"
              [itemContent]="getOutboundTypeLabel"
              [disabledItemHandler]="isOutboundTypeDisabled"
              [style.flex-direction]="'row'"
            />
          </fieldset>
          <tui-elastic-container>
            @if (outboundType() === 'vpn') {
              <tui-textfield tuiAnimated tuiChevron [stringify]="stringifyVpn">
                <label tuiLabel>{{ 'VPN client' | i18n }}</label>
                <input tuiSelect formControlName="outboundVpn" />
                <tui-data-list *tuiDropdown>
                  @for (vpn of outboundVpns; track vpn.interface) {
                    <button tuiOption [value]="vpn.interface">
                      {{ vpn.label }}
                    </button>
                  }
                </tui-data-list>
              </tui-textfield>
            }
          </tui-elastic-container>
          <fieldset>
            <legend>
              {{ 'Access' | i18n }}
              <button
                tuiButton
                size="xs"
                type="button"
                [disabled]="wanAccessType() === 'none'"
                (click)="onBlackout()"
              >
                {{ 'Blackout Schedule' | i18n }}
              </button>
            </legend>
            <tui-radio-list
              size="s"
              formControlName="wanAccessType"
              [items]="wanAccessOptions"
              [itemContent]="getWanAccessLabel"
              [style.flex-direction]="'row'"
            />
          </fieldset>
          <tui-elastic-container>
            @if (['whitelist', 'blacklist'].includes(wanAccessType())) {
              <tui-textfield tuiAnimated>
                @if (wanAccessType() === 'whitelist') {
                  <label tuiLabel>{{ 'Allowed IPs' | i18n }}</label>
                } @else {
                  <label tuiLabel>{{ 'Blocked IPs' | i18n }}</label>
                }
                <input
                  tuiInput
                  formControlName="wanAccessList"
                  [placeholder]="'e.g. 1.1.1.1, 8.8.8.8/24' | i18n"
                />
              </tui-textfield>
              <tui-error formControlName="wanAccessList" />
            }
          </tui-elastic-container>
        }
        @case (2) {
          <tui-radio-list
            size="s"
            formControlName="dnsMode"
            [items]="dnsModeOptions"
            [itemContent]="getDnsModeLabel"
            [style.flex-direction]="'row'"
          />
          <tui-elastic-container>
            @if (dnsMode() === 'custom') {
              @for (
                label of ['Primary', 'Secondary', 'Tertiary'];
                track $index
              ) {
                <section tuiAnimated>
                  <tui-textfield>
                    <label tuiLabel>
                      {{ label | i18n
                      }}{{ $first ? '' : ' ' + ('(optional)' | i18n) }}
                    </label>
                    <input tuiInput [formControlName]="'dns' + ($index + 1)" />
                  </tui-textfield>
                  <label tuiLabel>
                    <input
                      type="checkbox"
                      tuiSwitch
                      [formControlName]="'dns' + ($index + 1) + 'Tls'"
                    />
                    {{ 'Secure (DoH)' | i18n }}
                  </label>
                </section>
                <tui-error [formControlName]="'dns' + ($index + 1)" />
              }
            }
          </tui-elastic-container>
        }
      }

      <div class="blackout" [class._disabled]="wanAccessType() === 'none'">
        <label tuiLabel>{{ 'Blackout times' | i18n }}</label>
        <app-schedule [style.height.rem]="22" [(windows)]="scheduleWindows" />
      </div>

      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  styles: `
    tui-segmented button {
      flex: 1;
      font-weight: bold;
      min-inline-size: min-content;
    }

    legend [tuiButton] {
      margin-inline-start: 0.5rem;
      vertical-align: baseline;
    }

    .blackout {
      display: none;
      flex-direction: column;
      gap: inherit;
      transition: opacity 0.3s;
    }

    .blackout._disabled {
      opacity: var(--tui-disabled-opacity);
      pointer-events: none;
    }
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideHelp('/profiles/dialog'),
    provideTranslatedValidationErrors({
      required: 'Required',
      min: 'Must be at least 0',
      max: 'Must be at most 254',
      ipv4: 'Enter a valid IPv4 address',
      duplicateName: 'A profile with this name already exists',
      duplicateSubnet: 'This subnet is already in use by another profile',
      subnetLocked:
        'Cannot change subnet while devices have static IP reservations in this subnet',
      ipv4List:
        'Enter valid IPv4 addresses or CIDRs (e.g. 1.1.1.1, 8.8.8.8/24)',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiError,
    TuiButton,
    TuiInput,
    TuiLabel,
    TuiCheckbox,
    TuiRadioList,
    TuiInputNumber,
    TuiSelect,
    TuiChevron,
    TuiDataList,
    TuiSwitch,
    TuiGroup,
    TuiSelectLike,
    TuiInputChip,
    TuiMultiSelect,
    TuiDataListWrapper,
    FormsModule,
    TuiStringifyPipe,
    TuiElasticContainer,
    TuiAnimated,
    ScheduleComponent,
    TuiSegmented,
    i18nPipe,
  ],
})
class AddProfile {
  protected readonly context =
    injectContext<TuiDialogContext<ProfileDialogResult, ProfileDialogData>>()

  private readonly i18n = inject(i18nPipe)
  private readonly builder = inject(NonNullableFormBuilder)
  private readonly existing = this.context.data.existing
  private readonly usedSubnets = this.context.data.usedSubnets
  private readonly dnsConfig = this.parseDnsOverride()
  private readonly dialogs = inject(TuiResponsiveDialogService)

  protected readonly tab = signal(0)
  protected readonly outboundVpns = this.context.data.outboundVpns
  protected readonly subnetBase = computed(() => this.context.data.subnetBase)
  protected readonly scheduleWindows = signal<ScheduleWindow[]>(
    this.context.data.scheduleWindows ?? [],
  )

  protected readonly form = this.builder.group({
    fullname: [
      this.existing?.fullname || '',
      [
        Validators.required,
        CustomValidators.duplicateName(
          this.context.data.otherProfiles.map(p => p.fullname),
        ),
      ],
    ],
    subnet: [
      this.nextAvailableSubnet(),
      [
        Validators.required,
        Validators.min(0),
        Validators.max(254),
        CustomValidators.duplicateSubnet(this.usedSubnets),
      ],
    ],
    outboundType: [this.getOutboundType()],
    outboundVpn: [this.getOutboundVpn()],
    dnsMode: [this.dnsConfig.dnsMode],
    dns1: [this.dnsConfig.dns1, CustomValidators.ipv4()],
    dns1Tls: [this.dnsConfig.dns1Tls],
    dns2: [this.dnsConfig.dns2, CustomValidators.ipv4()],
    dns2Tls: [this.dnsConfig.dns2Tls],
    dns3: [this.dnsConfig.dns3, CustomValidators.ipv4()],
    dns3Tls: [this.dnsConfig.dns3Tls],
    lanAccessType: [this.getLanAccessType()],
    lanAccessProfiles: this.builder.group(this.buildLanAccessProfiles()),
    wanAccessType: [this.getWanAccessType()],
    wanAccessList: [this.getWanAccessList(), CustomValidators.ipv4List()],
    accessToNewProfiles: [this.existing?.access_to_new_profiles || false],
  })

  protected readonly multi = signal(
    this.context.data.otherProfiles.filter(
      profile =>
        this.form.controls.lanAccessProfiles.controls[profile.interface]?.value,
    ),
  )

  protected readonly lanAccessType = toSignal(
    this.form.controls.lanAccessType.valueChanges,
    { initialValue: this.form.controls.lanAccessType.value },
  )

  protected readonly wanAccessType = toSignal(
    this.form.controls.wanAccessType.valueChanges,
    { initialValue: this.form.controls.wanAccessType.value },
  )

  protected readonly outboundType = toSignal(
    this.form.controls.outboundType.valueChanges,
    { initialValue: this.form.controls.outboundType.value },
  )

  protected readonly dnsMode = toSignal(
    this.form.controls.dnsMode.valueChanges,
    { initialValue: this.form.controls.dnsMode.value },
  )

  constructor() {
    // Lock the subnet if devices in it have static IP reservations
    if (this.context.data.hasStaticIpsInSubnet) {
      this.form.controls.subnet.addValidators(control =>
        control.value === this.nextAvailableSubnet()
          ? null
          : { subnetLocked: true },
      )
      this.form.controls.subnet.updateValueAndValidity()
    }

    // Update dns field validators when DNS mode changes
    effect(() => {
      const useCustom = this.dnsMode() === 'custom'
      const { dns1, dns2, dns3 } = this.form.controls
      for (const control of [dns1, dns2, dns3]) {
        control.clearValidators()
        if (useCustom) {
          control.addValidators([CustomValidators.ipv4()])
        }
        control.updateValueAndValidity()
      }
      if (useCustom) {
        dns1.addValidators([Validators.required])
        dns1.updateValueAndValidity()
      }
    })

    // Require a profile selection when LAN access is whitelisted
    effect(() => {
      const needsSelection =
        this.lanAccessType() === 'whitelist' &&
        this.context.data.otherProfiles.length > 0
      const group = this.form.controls.lanAccessProfiles
      group.setValidators(needsSelection ? [atLeastOneSelected] : [])
      group.updateValueAndValidity()
    })

    // Require at least one entry when WAN access is whitelisted/blacklisted
    effect(() => {
      const type = this.wanAccessType()
      const control = this.form.controls.wanAccessList
      control.setValidators(
        type === 'whitelist' || type === 'blacklist'
          ? [CustomValidators.ipv4List(), nonEmptyList]
          : [CustomValidators.ipv4List()],
      )
      control.updateValueAndValidity()
    })
  }

  protected readonly outboundTypeOptions = ['direct', 'vpn'] as const
  protected readonly dnsModeOptions = ['system', 'custom'] as const
  protected readonly lanAccessOptions = ['all', 'none', 'whitelist']
  protected readonly wanAccessOptions = [
    'all',
    'none',
    'whitelist',
    'blacklist',
  ]

  protected readonly getOutboundTypeLabel: TuiStringHandler<
    TuiContext<string>
  > = ({ $implicit }) =>
    $implicit === 'vpn'
      ? this.i18n.transform('VPN')
      : this.i18n.transform('Direct')

  protected readonly isOutboundTypeDisabled = (value: string): boolean =>
    value === 'vpn' && !this.outboundVpns.length

  protected readonly getDnsModeLabel: TuiStringHandler<TuiContext<string>> = ({
    $implicit,
  }) =>
    $implicit === 'custom'
      ? this.i18n.transform('Custom')
      : this.i18n.transform('Inherit from system')

  protected readonly getLanAccessLabel: TuiStringHandler<TuiContext<string>> =
    ({ $implicit }) => {
      switch ($implicit) {
        case 'all':
          return this.i18n.transform('All')
        case 'none':
          return this.i18n.transform('Same profile')
        case 'whitelist':
          return this.i18n.transform('Whitelist')
        default:
          return $implicit
      }
    }

  protected readonly getWanAccessLabel: TuiStringHandler<TuiContext<string>> =
    ({ $implicit }) =>
      this.i18n.transform(
        $implicit.charAt(0).toUpperCase() + $implicit.slice(1).toLowerCase(),
      )

  protected onWhitelist(list: readonly SecurityProfile[]) {
    Object.entries(this.form.controls.lanAccessProfiles.controls).forEach(
      ([key, control]) => {
        control.setValue(list.some(item => item.interface === key))
      },
    )
  }

  private nextAvailableSubnet(): number {
    if (this.existing) {
      return parseInt(this.existing.gateway_ip.split('.')[2], 10)
    }
    for (let i = 1; i <= 254; i++) {
      if (!this.usedSubnets.includes(i)) return i
    }
    return 1
  }

  private getOutboundType(): 'direct' | 'vpn' {
    const outbound = this.existing?.outbound
    return outbound && outbound !== 'wan' ? 'vpn' : 'direct'
  }

  private getOutboundVpn(): string {
    const outbound = this.existing?.outbound
    if (outbound && outbound !== 'wan') return outbound
    return this.outboundVpns[0]?.interface ?? ''
  }

  private parseDnsOverride(): {
    dnsMode: 'system' | 'custom'
    dns1: string
    dns1Tls: boolean
    dns2: string
    dns2Tls: boolean
    dns3: string
    dns3Tls: boolean
  } {
    const servers = this.existing?.dns_override || []
    if (servers.length === 0) {
      return {
        dnsMode: 'system',
        dns1: '',
        dns1Tls: false,
        dns2: '',
        dns2Tls: false,
        dns3: '',
        dns3Tls: false,
      }
    }

    const s = (i: number) => servers[i] || { address: '', ssl: false }

    return {
      dnsMode: 'custom',
      dns1: s(0).address,
      dns1Tls: s(0).ssl,
      dns2: s(1).address,
      dns2Tls: s(1).ssl,
      dns3: s(2).address,
      dns3Tls: s(2).ssl,
    }
  }

  private getLanAccessType(): string {
    if (!this.existing) return 'all'
    const access = this.existing.lan_access
    if (access === 'ALL') return 'all'
    if (access === 'SAME_PROFILE') return 'none'
    if (typeof access === 'object' && 'other_profiles' in access) {
      return 'whitelist'
    }
    return 'all'
  }

  private buildLanAccessProfiles(): Record<string, boolean> {
    const result: Record<string, boolean> = {}
    const selectedProfiles = this.existing?.lan_access
    const selected =
      typeof selectedProfiles === 'object' &&
      'other_profiles' in selectedProfiles
        ? selectedProfiles.other_profiles
        : []

    for (const profile of this.context.data.otherProfiles) {
      // Check if this profile is in the selected list by matching interface
      result[profile.interface] = selected.some(
        (s: any) => s.interface === profile.interface,
      )
    }
    return result
  }

  private getWanAccessType(): string {
    if (!this.existing) return 'all'
    const access = this.existing.wan_access
    if (access === 'ALL') return 'all'
    if (access === 'NONE') return 'none'
    if (typeof access === 'object' && 'whitelist' in access) return 'whitelist'
    if (typeof access === 'object' && 'blacklist' in access) return 'blacklist'
    return 'all'
  }

  private getWanAccessList(): string {
    if (!this.existing) return ''
    const access = this.existing.wan_access
    if (typeof access === 'object' && 'whitelist' in access) {
      return access.whitelist.join(', ')
    }
    if (typeof access === 'object' && 'blacklist' in access) {
      return access.blacklist.join(', ')
    }
    return ''
  }

  protected readonly stringifyVpn = (value: string): string =>
    this.outboundVpns.find(v => v.interface === value)?.label ?? value

  protected onBlackout(): void {
    this.dialogs
      .open<ScheduleWindow[]>(PROFILE_SCHEDULE, {
        label: this.i18n.transform('Blackout Schedule'),
        size: 'l',
        data: this.scheduleWindows(),
      })
      .subscribe(windows => this.scheduleWindows.set(windows))
  }

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      const val = this.form.getRawValue()

      // Build LAN access
      let lan_access: LanAccess<ProfileIdOpt>
      if (val.lanAccessType === 'all') {
        lan_access = 'ALL'
      } else if (val.lanAccessType === 'none') {
        lan_access = 'SAME_PROFILE'
      } else {
        // whitelist
        const selectedProfiles = this.context.data.otherProfiles
          .filter(p => val.lanAccessProfiles[p.interface])
          .map(p => ({
            fullname: p.fullname,
            interface: p.interface,
            vlan_tag: p.vlan_tag,
          }))
        lan_access = { other_profiles: selectedProfiles }
      }

      // Build WAN access
      let wan_access: WanAccess
      if (val.wanAccessType === 'all') {
        wan_access = 'ALL'
      } else if (val.wanAccessType === 'none') {
        wan_access = 'NONE'
      } else if (val.wanAccessType === 'whitelist') {
        const ips = val.wanAccessList
          .split(',')
          .map(s => s.trim())
          .filter(Boolean)
        wan_access = { whitelist: ips }
      } else {
        // blacklist
        const ips = val.wanAccessList
          .split(',')
          .map(s => s.trim())
          .filter(Boolean)
        wan_access = { blacklist: ips }
      }

      // Build DNS override
      let dns_override: DnsServer[] | undefined
      if (val.dnsMode === 'custom') {
        const servers: DnsServer[] = []
        if (val.dns1) {
          servers.push({ address: val.dns1, ssl: val.dns1Tls })
        }
        if (val.dns2) {
          servers.push({ address: val.dns2, ssl: val.dns2Tls })
        }
        if (val.dns3) {
          servers.push({ address: val.dns3, ssl: val.dns3Tls })
        }
        dns_override = servers.length ? servers : undefined
      }

      const subnet = this.subnetBase()
      this.context.completeWith({
        fullname: val.fullname,
        gateway_ip: `${subnet.firstOctet}.${subnet.secondOctet}.${val.subnet}.1`,
        outbound: val.outboundType === 'vpn' ? val.outboundVpn : 'wan',
        lan_access,
        wan_access,
        access_to_new_profiles: val.accessToNewProfiles,
        owns_lan: this.existing?.owns_lan || false,
        dns_override: dns_override?.length ? dns_override : undefined,
        schedule_windows: this.scheduleWindows(),
      })
    }
  }
}

export const ADD_PROFILE = new PolymorpheusComponent(AddProfile)
