import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { StoreIconDirective } from '@start9labs/marketplace'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiLoader, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { TitleDirective } from 'src/app/services/title.service'
import { CONFIRM_REGISTRY } from './confirm'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Manage Registries' | i18n }}
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Manage Registries' | i18n }}
        <button tuiButton size="xs" iconStart="@tui.plus" (click)="add()">
          {{ 'Add registry' | i18n }}
        </button>
      </header>
      <p>
        {{
          'Connect registries to browse their packages and review the keys they ask you to trust.'
            | i18n
        }}
      </p>
      @if (registries(); as registries) {
        <table [appTable]="['Name', 'URL', null]">
          @for (registry of registries; track registry.url) {
            <tr>
              <td>
                <div tuiCell="s">
                  <span tuiAvatar size="s">
                    <img [storeIcon]="registry.url" />
                  </span>
                  <span tuiTitle>{{ registry.name }}</span>
                </div>
              </td>
              <td [attr.data-label]="'URL' | i18n">{{ registry.url }}</td>
              <td>
                <button
                  tuiButton
                  size="s"
                  appearance="outline-grayscale"
                  [disabled]="saving()"
                  (click)="review(registry.url)"
                >
                  {{ 'Review keys' | i18n }}
                </button>
                <button
                  tuiIconButton
                  size="s"
                  appearance="flat-grayscale"
                  iconStart="@tui.trash"
                  [disabled]="saving()"
                  (click)="remove(registry.url)"
                >
                  {{ 'Remove registry' | i18n }} {{ registry.name }}
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="3">{{ 'No registries connected' | i18n }}</td>
            </tr>
          }
        </table>
      } @else {
        <tui-loader />
      }
    </section>
  `,
  styles: `
    header button {
      margin-inline-start: auto;
    }
    section > p {
      margin: 1rem;
      color: var(--tui-text-secondary);
    }
    td:last-child {
      text-align: end;
    }
    :host-context(tui-root._mobile) {
      td[data-label]::before {
        content: attr(data-label) ': ';
        color: var(--tui-text-secondary);
      }
      td:last-child {
        grid-area: 1 / 2 / 3;
        align-content: center;
      }
    }
  `,
  imports: [
    RouterLink,
    StoreIconDirective,
    TableComponent,
    TitleDirective,
    TuiAvatar,
    TuiButton,
    TuiCell,
    TuiLoader,
    TuiTitle,
    i18nPipe,
  ],
})
export default class Registries {
  private readonly marketplace = inject(MarketplaceService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

  protected readonly registries = toSignal(this.marketplace.registries$)
  protected readonly saving = signal(false)

  protected async add(): Promise<void> {
    const url = await firstValueFrom(
      this.dialogs
        .openPrompt<string>({
          label: 'Add registry',
          data: {
            message: 'The domain or URL of the custom registry',
            label: 'URL',
            placeholder: 'e.g. registry.example.com',
            buttonText: 'Continue',
          },
        })
        .pipe(defaultIfEmpty('')),
    )
    if (url) await this.review(url)
  }

  protected async review(url: string): Promise<void> {
    let preview:
      | Awaited<ReturnType<MarketplaceService['previewRegistry']>>
      | undefined
    await this.tasks.run(async () => {
      preview = await this.marketplace.previewRegistry(url)
    }, 'Validating registry')
    if (!preview) return
    const confirmed = await firstValueFrom(
      this.dialogs
        .openComponent<boolean>(CONFIRM_REGISTRY, { data: preview })
        .pipe(defaultIfEmpty(false)),
    )
    if (!confirmed) return
    const { url: registryUrl, info } = preview
    this.saving.set(true)
    await this.tasks.run(
      () => this.marketplace.add(registryUrl, Object.keys(info.trustKeys)),
      'Saving',
    )
    this.saving.set(false)
  }

  protected async remove(url: string): Promise<void> {
    const confirmed = await firstValueFrom(
      this.dialogs
        .openConfirm({
          label: 'Remove registry?',
          data: {
            content:
              'Its keys will remain in Manage Trust with their existing permissions.',
            yes: 'Remove',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false)),
    )
    if (!confirmed) return
    this.saving.set(true)
    await this.tasks.run(() => this.marketplace.delete(url), 'Deleting')
    this.saving.set(false)
  }
}
