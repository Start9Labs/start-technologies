import { Component, computed } from '@angular/core'
import { Summary } from 'src/app/components/summary'
import { injectFormService } from 'src/app/services/form.service'
import { MacForm, MAC_LABELS } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[macSummary]',
  template: `
    <section>
      @if (mac(); as mac) {
        <div [appSummary]="mac">{{ labels.mac | i18n }}</div>
      }
    </section>
  `,
  imports: [Summary, i18nPipe],
})
export class MacSummary {
  protected readonly service = injectFormService<MacForm>()
  protected readonly labels = MAC_LABELS

  readonly mac = computed(() => this.service.data()?.address.mac)
}
