import { Directive, inject, input } from '@angular/core'
import {
  convertAnsi,
  DownloadHTMLService,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { LogsComponent } from './logs.component'

@Directive({
  selector: 'button[logsDownload]',
  host: { '(click)': 'download()' },
})
export class LogsDownloadDirective {
  private readonly component = inject(LogsComponent)
  private readonly tasks = inject(TaskService)
  private readonly downloadHtml = inject(DownloadHTMLService)

  logsDownload =
    input.required<(params: T.LogsParamsInput) => Promise<T.LogResponse>>()

  async download() {
    this.tasks.run(async () => {
      const { entries } = await this.logsDownload()({
        before: true,
        limit: 10000,
      })

      this.downloadHtml.download(
        `${this.component.context}-logs.html`,
        convertAnsi(entries),
        STYLES,
      )
    }, 'Processing 10,000 logs')
  }
}

const STYLES = {
  'background-color': '#222428',
  color: '#e0e0e0',
  'font-family': 'monospace',
}
