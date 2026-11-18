import { Component, Directive, inject, input } from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import {
  TUI_ICON_START,
  TuiButton,
  tuiButtonOptionsProvider,
} from '@taiga-ui/core'
import { MarketplaceService } from 'src/app/services/marketplace.service'

/** Saved registries remain manually refreshable in every connection state. */
@Directive({
  selector: 'button[registryRefresh]',
  host: { type: 'button', '(click)': 'refresh()' },
})
export class RegistryRefresh {
  private readonly marketplace = inject(MarketplaceService)
  private readonly tasks = inject(TaskService)

  public readonly registryRefresh = input.required<string>()

  protected async refresh(): Promise<void> {
    const url = this.registryRefresh()
    await this.tasks.run(
      () => this.marketplace.refreshRegistry(url),
      'Checking registry',
    )
  }
}

@Component({
  selector: 'button[registryRetry]',
  template: '{{ "Refresh registry" | i18n }}',
  host: { tuiIconButton: '' },
  hostDirectives: [
    { directive: TuiButton, inputs: ['size'] },
    { directive: RegistryRefresh, inputs: ['registryRefresh: registryRetry'] },
  ],
  providers: [
    tuiButtonOptionsProvider({ size: 's', appearance: 'flat-grayscale' }),
    { provide: TUI_ICON_START, useValue: '@tui.refresh-cw' },
  ],
  imports: [i18nPipe],
})
export class RegistryRetry {}
