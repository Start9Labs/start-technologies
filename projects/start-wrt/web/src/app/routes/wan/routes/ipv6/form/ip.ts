import { Component, computed, inject } from '@angular/core'
import { ReactiveFormsModule } from '@angular/forms'
import { MaskitoDirective } from '@maskito/angular'
import { TUI_FALSE_HANDLER, TuiAnimated } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiHint,
  TuiInput,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiRadioList } from '@taiga-ui/kit'
import {
  TuiCardLarge,
  TuiElasticContainer,
  TuiForm,
  TuiHeader,
} from '@taiga-ui/layout'
import { FORM } from 'src/app/components/form'
import { PREFIX } from 'src/app/utils/masks'
import WanIpv6 from '../'
import {
  IPV6_ALL_CONTROLS,
  IPV6_CONTROLS,
  IPV6_LABELS,
  IPV6_MODES,
  IPV6_VALIDATION_ERRORS,
} from '../utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'wan-ipv6-ip',
  template: `
    <header tuiHeader="body-l">
      <h2 tuiTitle>{{ 'IP Address' | i18n }}</h2>
    </header>
    <tui-radio-list
      size="s"
      formControlName="mode"
      [items]="modes"
      [itemContent]="template"
      [disabledItemHandler]="handler()"
    />
    <ng-template #template let-item>
      {{ $any(labels)[item] | i18n
      }}{{ item === 'slaac' ? (' (Default)' | i18n) : '' }}
      <i [tuiHint]="'Published ports are using IPv6' | i18n"></i>
    </ng-template>
    <tui-elastic-container>
      @if (parent.ipMode() !== 'disabled') {
        <section tuiAnimated>
          @for (name of all; track name) {
            @let auto = ['slaac', 'dhcpv6'].includes(parent.ipMode());
            @let optional =
              (name === 'prefix' && auto) || name === 'lan_prefix';

            @if (controls[parent.ipMode()]?.includes(name)) {
              <div>
                <tui-textfield>
                  <label tuiLabel>
                    {{ labels[name] | i18n
                    }}{{ optional ? (' (optional)' | i18n) : '' }}
                  </label>
                  <input
                    tuiInput
                    [formControlName]="name"
                    [maskito]="
                      ['prefix', 'ip6prefixlen', 'ip4prefixlen'].includes(name)
                        ? mask
                        : null
                    "
                    [placeholder]="optional ? ('Auto' | i18n) : ''"
                  />
                </tui-textfield>
                <tui-error [formControlName]="name" />
              </div>
            }
          }
        </section>
      }
    </tui-elastic-container>
  `,
  styles: `
    tui-radio-list {
      flex-direction: row;
      gap: inherit;
    }
  `,
  viewProviders: [FORM],
  hostDirectives: [TuiForm, TuiCardLarge],
  providers: [provideTranslatedValidationErrors(IPV6_VALIDATION_ERRORS)],
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiTextfield,
    TuiRadioList,
    TuiError,
    TuiInput,
    TuiHint,
    MaskitoDirective,
    TuiAnimated,
    TuiElasticContainer,
    i18nPipe,
  ],
})
export class WanIpv6Ip {
  protected readonly parent = inject(WanIpv6)
  protected readonly modes = IPV6_MODES
  protected readonly labels = IPV6_LABELS
  protected readonly controls = IPV6_CONTROLS
  protected readonly all = IPV6_ALL_CONTROLS
  protected readonly mask = PREFIX
  protected readonly handler = computed(() =>
    this.parent.hasIpv6Ports()
      ? (item: string) => item === 'disabled'
      : TUI_FALSE_HANDLER,
  )
}
