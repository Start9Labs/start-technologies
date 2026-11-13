import { Component, inject, input } from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import {
  TUI_ICON_START,
  TuiButton,
  tuiButtonOptionsProvider,
} from '@taiga-ui/core'
import { MarketplaceService } from 'src/app/services/marketplace.service'

@Component({
  selector: 'button[registryRetry]',
  template: '{{ "Refresh registry" | i18n }}',
  host: { tuiIconButton: '', type: 'button', '(click)': 'retry()' },
  hostDirectives: [{ directive: TuiButton, inputs: ['size'] }],
  providers: [
    tuiButtonOptionsProvider({ size: 's', appearance: 'flat-grayscale' }),
    { provide: TUI_ICON_START, useValue: '@tui.refresh-cw' },
  ],
  imports: [i18nPipe],
})
export class RegistryRetry {
  private readonly marketplace = inject(MarketplaceService)
  private readonly tasks = inject(TaskService)

  public readonly registryRetry = input.required<string>()

  protected async retry(): Promise<void> {
    const url = this.registryRetry()
    await this.tasks.run(
      () => this.marketplace.refreshRegistry(url),
      'Checking registry',
    )
  }
}
