import { Component, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiLink, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { RegistryRefresh } from './registry-retry'

@Component({
  selector: 'registry-notice',
  template: `
    <div tuiNotification appearance="info">
      <h3 tuiTitle>
        {{ 'Listings may be out of date' | i18n }}
        <span tuiSubtitle>
          {{ "StartOS can't reach this registry right now." | i18n }}
        </span>
        <span>
          <button tuiButton [registryRefresh]="url()">
            {{ 'Retry' | i18n }}
          </button>
          <a tuiLink routerLink="/system/registries">
            {{ 'Manage Registries' | i18n }}
          </a>
        </span>
      </h3>
    </div>
  `,
  host: { style: 'display: block' },
  imports: [
    RouterLink,
    TuiButton,
    TuiLink,
    TuiNotification,
    TuiTitle,
    RegistryRefresh,
    i18nPipe,
  ],
})
export class RegistryNotice {
  public readonly url = input.required<string>()
}
