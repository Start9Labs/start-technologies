import { inject, Service } from '@angular/core'
import { tuiIsString } from '@taiga-ui/cdk'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { PolymorpheusContent } from '@taiga-ui/polymorpheus'

import { i18nPipe } from '../i18n/i18n.pipe'
import { ErrorService } from './error.service'

@Service()
export class TaskService {
  private readonly loader = inject(TuiNotificationMiddleService)
  private readonly error = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)

  async run(task: Function, content?: PolymorpheusContent): Promise<boolean> {
    const message = tuiIsString(content)
      ? this.i18n.transform(content)
      : content
    const loader = this.loader.open(message ?? '').subscribe()

    try {
      return await this.runInline(task)
    } finally {
      loader.unsubscribe()
    }
  }

  async runInline(task: Function): Promise<boolean> {
    try {
      await task()

      return true
    } catch (e: any) {
      this.error.handleError(e)

      return false
    }
  }
}
