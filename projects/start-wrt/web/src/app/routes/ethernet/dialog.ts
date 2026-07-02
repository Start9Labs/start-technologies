import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiButton, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import {
  TuiRadioList,
  TuiStringifyContentPipe,
  TuiStringifyPipe,
} from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

import type { EthernetPortView } from './service'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Change WAN Port' | i18n }}</h2>
        <p>
          {{
            'Select which port should become the WAN port. This will restart the network and may briefly interrupt your connection.'
              | i18n
          }}
        </p>
      </hgroup>
    </header>
    <tui-radio-list
      [itemContent]="'name' | tuiStringify | tuiStringifyContent"
      [items]="context.data"
      [(ngModel)]="selected"
    />
    <footer>
      <button
        tuiButton
        appearance="flat"
        (click)="context.$implicit.complete()"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        [disabled]="!current || current === this.selected"
        (click)="context.completeWith(selected)"
      >
        {{ 'Change and Restart' | i18n }}
      </button>
    </footer>
  `,
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/ethernet/dialog')],
  imports: [
    FormsModule,
    TuiButton,
    TuiRadioList,
    TuiHeader,
    TuiTitle,
    TuiStringifyPipe,
    TuiStringifyContentPipe,
    i18nPipe,
  ],
})
export class ChangeWanDialog {
  readonly context =
    injectContext<
      TuiDialogContext<EthernetPortView | undefined, EthernetPortView[]>
    >()

  current = this.context.data.find(p => p.wan)
  selected = this.current
}

export const CHANGE_WAN_DIALOG = new PolymorpheusComponent(ChangeWanDialog)
