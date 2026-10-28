import { Directive, HostListener, inject, Input } from '@angular/core'
import { DialogService, i18nKey, TaskService } from '@start9labs/shared'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { SnakeComponent } from './snake.component'
import { pathFromArray } from 'patch-db-client'

@Directive({ selector: 'img[snake]' })
export class SnakeDirective {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)

  @Input()
  snake = 0

  @HostListener('click')
  async onClick() {
    this.dialog
      .openComponent<number>(new PolymorpheusComponent(SnakeComponent), {
        label: 'Snake!' as i18nKey,
        size: 'l',
        closable: false,
        dismissible: false,
        data: this.snake,
      })
      .pipe(filter(score => score > this.snake))
      .subscribe(score =>
        this.tasks.run(
          async () =>
            await this.api.rpc.db.put.ui({
              pointer: pathFromArray(['snakeHighScore']),
              value: score,
            }),
          'Saving high score',
        ),
      )
  }
}
