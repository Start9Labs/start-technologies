import { Component, inject } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { provideFormService } from 'src/app/services/form.service'

import { i18nPipe } from 'src/app/i18n/i18n.pipe'

import { EthernetTable } from './table'
import { CHANGE_WAN_DIALOG } from './dialog'
import { EthernetPortView, EthernetService } from './service'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Ethernet' | i18n }}</h2>
      </hgroup>
      @if (service.data(); as data) {
        <aside tuiAccessories>
          <button tuiButton size="s" (click)="onChangeWan(data)">
            {{ 'Change WAN Port' | i18n }}
          </button>
        </aside>
      }
    </header>
    <table
      [tuiSkeleton]="!service.data()"
      [ethernetTable]="service.data() || []"
    ></table>
  `,
  styles: `
    :host {
      max-width: 36rem;
    }
  `,
  host: { class: 'g-page' },
  providers: [provideFormService(EthernetService)],
  imports: [
    TuiHeader,
    TuiTitle,
    TuiButton,
    TuiSkeleton,
    EthernetTable,
    i18nPipe,
  ],
})
export default class Ethernet {
  private readonly dialogs = inject(TuiResponsiveDialogService)

  protected readonly service = inject(EthernetService)

  onChangeWan(data: EthernetPortView[]) {
    this.dialogs
      .open<EthernetPortView | null>(CHANGE_WAN_DIALOG, { size: 's', data })
      .pipe(filter(Boolean))
      .subscribe(async ({ name }) => {
        const items = data.map(port => ({
          ...port,
          wan: port.name === name,
          profile: port.name === name ? null : port.profile,
        }))

        await this.service.save(items)
      })
  }
}
