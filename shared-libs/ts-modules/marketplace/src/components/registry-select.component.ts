import { Component, computed, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router, RouterLink } from '@angular/router'
import {
  DialogService,
  i18nPipe,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiHint, TuiIcon } from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { TuiAsideComponent, TuiNavigation } from '@taiga-ui/layout'
import { defaultIfEmpty, filter, firstValueFrom } from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { StoreIconDirective } from './store-icon.directive'

@Component({
  selector: 'marketplace-registry-select',
  template: `
    @if (aside.expanded()) {
      <h3 class="g-title">
        {{ 'Registries' | i18n }}
        @if (manageRegistriesLink(); as link) {
          <a
            tuiIconButton
            size="xs"
            appearance="icon"
            iconStart="@tui.settings"
            [routerLink]="link"
          >
            {{ 'Manage' | i18n }}
          </a>
        } @else {
          <button tuiButton size="xs" iconStart="@tui.plus" (click)="add()">
            {{ 'Add' | i18n }}
          </button>
        }
      </h3>
    }
    @if (data(); as d) {
      @for (registry of d.registries; track registry.url) {
        <button
          tuiAsideItem
          type="button"
          [class._selected]="registry.selected"
          [class._active]="registry.selected && !aside.expanded()"
          (click)="connect(registry.url)"
          (keydown.backspace)="registry.custom && delete(registry.url)"
          (keydown.delete)="registry.custom && delete(registry.url)"
        >
          <span tuiAvatar size="xs"><img [storeIcon]="registry.url" /></span>
          <span tuiFade>{{ registry.name }}</span>
          @if (registry.custom) {
            <tui-icon
              icon="@tui.trash"
              [tuiHint]="'Delete' | i18n"
              (click.stop)="delete(registry.url)"
            />
          }
        </button>
      }
      @if (d.unsaved; as url) {
        <button
          tuiAsideItem
          type="button"
          class="_selected"
          [class._active]="!aside.expanded()"
        >
          <span tuiAvatar size="xs"><img [storeIcon]="url" /></span>
          <span tuiFade>{{ fetched()?.info?.name || url }}</span>
          <tui-icon
            icon="@tui.bookmark"
            [tuiHint]="'Save this registry' | i18n"
            (click.stop)="saveCurrent(url)"
          />
        </button>
      }
    }
  `,
  styles: `
    :host {
      display: contents;
    }

    .g-title {
      justify-content: space-between;
      margin: 0.25rem 0 0.5rem;
      padding-inline-start: 0.5rem;
    }

    ._selected {
      font-weight: bold;
    }

    [tuiAvatar] {
      margin-inline-start: -0.25rem;
    }

    tui-icon {
      margin-inline-start: auto;
      font-size: 1rem;
      color: var(--tui-text-tertiary);
    }
  `,
  imports: [
    RouterLink,
    StoreIconDirective,
    TuiAvatar,
    TuiButton,
    TuiFade,
    TuiHint,
    TuiIcon,
    TuiNavigation,
    i18nPipe,
  ],
})
export class MarketplaceRegistrySelectComponent {
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)
  private readonly marketplace = inject(AbstractMarketplaceService)
  private readonly router = inject(Router)

  protected readonly aside = inject(TuiAsideComponent)
  protected readonly fetched = toSignal(this.marketplace.currentRegistry$)

  public readonly manageRegistriesLink = input('')

  private readonly registries = toSignal(this.marketplace.registries$)
  private readonly currentUrl = toSignal(this.marketplace.currentRegistryUrl$)

  protected readonly data = computed(
    (registries = this.registries(), currentUrl = this.currentUrl()) =>
      registries && {
        registries: registries.map((registry, index) => ({
          ...registry,
          selected: sameUrl(registry.url, currentUrl),
          custom: !this.manageRegistriesLink() && index > 1,
        })),
        unsaved:
          this.manageRegistriesLink() ||
          registries.some(({ url }) => sameUrl(url, currentUrl))
            ? null
            : currentUrl,
      },
  )

  async connect(url: string): Promise<void> {
    if (sameUrl(url, this.currentUrl())) return

    this.tasks.run(async () => {
      await this.marketplace.connect(url)
      this.router.navigate([], {
        queryParams: { registry: url },
        queryParamsHandling: 'merge',
      })
    }, 'Changing registry')
  }

  async add(): Promise<void> {
    const rawUrl = await firstValueFrom(
      this.dialog
        .openPrompt<string>({
          label: 'Add Custom Registry',
          data: {
            message: 'The domain or URL of the custom registry',
            label: 'URL',
            placeholder: 'e.g. registry.example.com',
            buttonText: 'Save',
          },
        })
        .pipe(defaultIfEmpty('')),
    )

    if (!rawUrl) return

    this.tasks.run(
      async () => await this.connect(await this.marketplace.add(rawUrl)),
      'Validating registry',
    )
  }

  saveCurrent(url: string): void {
    this.tasks.run(async () => await this.marketplace.add(url), 'Saving')
  }

  delete(url: string): void {
    this.dialog
      .openConfirm({
        label: 'Confirm',
        size: 's',
        data: {
          content: 'Are you sure you want to delete this registry?',
          yes: 'Delete',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () => await this.marketplace.delete(url),
          'Deleting',
        ),
      )
  }
}
