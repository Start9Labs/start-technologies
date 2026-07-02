import { Component, computed, effect, inject, signal } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiError,
  TuiHint,
  TuiInput,
  TuiLink,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TUI_CONFIRM,
  TuiChevron,
  TuiDataListWrapper,
  TuiInputNumber,
  TuiSelect,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { OutboundService } from 'src/app/routes/outbound/service'
import {
  getOutboundVpnForm,
  getSafeTargets,
  OUTBOUND_VALIDATION_ERRORS,
} from 'src/app/routes/outbound/utils'
import { CustomValidators } from 'src/app/utils/validators'
import { VPNSummary } from './summary'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>
          <a
            tuiLink
            routerLink=".."
            appearance=""
            iconStart="@tui.chevron-left"
            [tuiSkeleton]="!data()"
            [style.font]="'inherit'"
            [style.text-decoration]="'none'"
          >
            {{ data()?.label || ('Outbound VPN' | i18n) }}
          </a>
        </h2>
      </hgroup>
    </header>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article vpnSummary [formLoading]="!data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!data()"
      (reset.prevent)="onCancel()"
      (ngSubmit)="onSave()"
    >
      <section>
        <div>
          <tui-textfield>
            <label tuiLabel>{{ 'Label' | i18n }}</label>
            <input tuiInput formControlName="label" />
          </tui-textfield>
          <tui-error formControlName="label" />
        </div>
      </section>
      <section>
        <div>
          <tui-textfield tuiChevron [stringify]="stringifyTarget">
            <label tuiLabel>{{ 'Connects to' | i18n }}</label>
            <input tuiSelect formControlName="target" />
            <tui-data-list-wrapper *tuiDropdown [items]="targetOptions()" />
          </tui-textfield>
        </div>
      </section>
      <section>
        <div>
          <tui-textfield>
            <label tuiLabel>{{ 'MTU' | i18n }}</label>
            <input
              tuiInputNumber
              [placeholder]="'Default (~1420)' | i18n"
              formControlName="mtu"
            />
          </tui-textfield>
          <tui-error formControlName="mtu" />
          <small class="g-secondary">
            {{
              'Leave blank to use the default. Lower to 1280 if the VPN connects but requests time out.'
                | i18n
            }}
          </small>
        </div>
      </section>
      @if (data()) {
        <footer appFooter>
          <button
            tuiButton
            type="button"
            appearance="secondary-destructive"
            class="g-delete"
            [disabled]="dependentVpns().length"
            [tuiHint]="
              dependentVpns().length
                ? ('Cannot delete:' | i18n) +
                  ' ' +
                  dependentVpns().join(', ') +
                  ' ' +
                  (dependentVpns().length === 1
                    ? ('uses' | i18n)
                    : ('use' | i18n)) +
                  ' ' +
                  ('this VPN as a target. Change' | i18n) +
                  ' ' +
                  (dependentVpns().length === 1
                    ? ('its' | i18n)
                    : ('their' | i18n)) +
                  ' ' +
                  ('target first.' | i18n)
                : null
            "
            (click)="onDelete()"
          >
            {{ 'Delete' | i18n }}
          </button>
        </footer>
      }
    </form>
  `,
  host: { class: 'g-page' },
  providers: [
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
    provideTranslatedValidationErrors(OUTBOUND_VALIDATION_ERRORS),
  ],
  imports: [
    RouterLink,
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiLink,
    TuiTextfield,
    TuiError,
    TuiChevron,
    TuiSelect,
    TuiDataListWrapper,
    TuiButton,
    TuiHint,
    TuiInput,
    TuiInputNumber,
    Footer,
    Form,
    VPNSummary,
    TuiSkeleton,
    i18nPipe,
  ],
})
export default class OutboundVPN {
  private readonly route = inject(ActivatedRoute)
  private readonly router = inject(Router)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  readonly service = inject(OutboundService)
  readonly vpnId = this.route.snapshot.queryParams['id']
  private readonly otherLabels = computed(() => {
    const allVpns = this.service.data() ?? []
    return allVpns.filter(v => v.id !== this.vpnId).map(v => v.label)
  })
  readonly form = getOutboundVpnForm(inject(NonNullableFormBuilder), [])
  readonly data = computed(
    () => this.service.data()?.find(v => v.id === this.vpnId) ?? null,
  )

  readonly dependentVpns = computed(() => {
    const allVpns = this.service.data() ?? []
    const thisLabel = this.data()?.label
    if (!thisLabel) return []
    return allVpns.filter(v => v.target === thisLabel).map(v => v.label)
  })

  readonly targetOptions = computed(() => {
    const currentLabel = this.data()?.label
    if (!currentLabel) return ['Internet']
    return getSafeTargets(currentLabel, this.service.data() ?? [])
  })

  // Translates the 'Internet' option; user VPN labels pass through unchanged.
  protected readonly stringifyTarget = (v: string): string =>
    this.i18n.transform(v)

  constructor() {
    effect(() => {
      const data = this.data()
      const otherLabels = this.otherLabels()
      if (data && this.form.pristine) {
        this.form.controls.label.setValidators([
          Validators.required,
          CustomValidators.duplicateName(otherLabels),
          CustomValidators.interfaceNameLength('wg_', 15),
        ])
        this.form.reset({ ...data })
      }
    })
  }

  async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }

    const success = await this.service.update(
      this.vpnId,
      this.form.getRawValue(),
    )
    if (success) {
      this.form.markAsPristine()
      this.router.navigate(['..'], { relativeTo: this.route })
    }
  }

  onCancel() {
    const data = this.data()
    if (data) {
      this.form.reset({ ...data })
    }
  }

  onDelete() {
    const usedBy = this.data()?.used_by ?? []
    const label = usedBy.length
      ? this.i18n.transform('Delete VPN?')
      : this.i18n.transform('Are you sure?')
    const data = usedBy.length
      ? {
          content: `${this.i18n.transform('The following profiles currently route through this VPN and will be switched to WAN:')} ${usedBy.join(', ')}.`,
          yes: this.i18n.transform('Delete'),
          no: this.i18n.transform('Cancel'),
        }
      : undefined

    this.dialogs
      .open(TUI_CONFIRM, { label, data })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        if (await this.service.remove(this.vpnId)) {
          this.router.navigate(['..'], { relativeTo: this.route })
        }
      })
  }
}
