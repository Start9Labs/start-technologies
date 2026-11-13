import { NgTemplateOutlet } from '@angular/common'
import { Component, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router, RouterLink } from '@angular/router'
import {
  DialogService,
  i18nPipe,
  isDefaultRegistry,
  registryUrl,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import {
  TUI_BREAKPOINT,
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { combineLatest, defaultIfEmpty, firstValueFrom, map } from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { StoreIconDirective } from './store-icon.directive'

@Component({
  selector: 'marketplace-registry-select',
  template: `
    <button
      tuiButton
      tuiDropdown
      iconEnd="@tui.chevron-right"
      [tuiDropdownSided]="breakpoint() !== 'mobile'"
      size="s"
      appearance="flat-grayscale"
      [(tuiDropdownOpen)]="open"
    >
      <span tuiAvatar appearance="action-grayscale" size="xs">
        <img [storeIcon]="data()?.current?.url || data()?.currentUrl" />
      </span>
      <b tuiFade>
        {{ data()?.current?.name || fetched()?.info?.name || 'Loading...' }}
      </b>
      @if (data(); as d) {
        <tui-data-list *tuiDropdown size="m">
          <div class="g-title">
            @if (manageRegistriesLink()) {
              <small>{{ 'Saved Registries' | i18n }}</small>
              <a
                tuiIconButton
                size="xs"
                appearance="icon"
                iconStart="@tui.settings"
                [routerLink]="manageRegistriesLink()"
                (click)="open = false"
              >
                {{ 'Manage Registries' | i18n }}
              </a>
            } @else {
              <small>{{ 'Registries' | i18n }}</small>
            }
          </div>
          @for (registry of d.standard; track registry.url) {
            <ng-container
              *ngTemplateOutlet="row; context: { $implicit: registry }"
            />
          }
          @if (manageRegistriesLink()) {
            @for (registry of d.custom; track registry.url) {
              @if ($first) {
                <hr />
              }
              <ng-container
                *ngTemplateOutlet="row; context: { $implicit: registry }"
              />
            }
          } @else {
            <hr />
            <div class="g-title">
              <small>{{ 'Custom' | i18n }}</small>
              @if (d.custom[0]; as custom) {
                <button
                  tuiButton
                  size="xs"
                  appearance="secondary-grayscale"
                  (click)="viewCustom(custom.url)"
                >
                  {{ 'Change' | i18n }}
                </button>
              }
            </div>
            @for (registry of d.custom; track registry.url) {
              <ng-container
                *ngTemplateOutlet="row; context: { $implicit: registry }"
              />
            } @empty {
              <button tuiOption (click)="viewCustom()">
                <tui-icon icon="@tui.search" />
                {{ 'Enter a domain or URL' | i18n }}
              </button>
            }
          }
        </tui-data-list>
      }
    </button>
    <ng-template #row let-registry>
      <button tuiOption (click)="connect(registry.url)">
        <span tuiAvatar><img [storeIcon]="registry.url" /></span>
        <span tuiTitle>
          {{ registry.name }}
          <span tuiSubtitle>{{ registry.url }}</span>
        </span>
        @if (registry.selected) {
          <tui-icon icon="@tui.check" />
        }
      </button>
    </ng-template>
  `,
  styles: `
    :host {
      display: grid;
      margin-block-end: 0.75rem;

      [tuiAvatar] {
        margin-inline-start: -0.375rem;
      }

      [tuiButton] {
        justify-content: flex-start;

        &::after {
          margin-inline-start: auto;
        }
      }
    }

    tui-icon {
      font-size: 1rem;
    }

    .g-title {
      justify-content: space-between;
      margin: 0.5rem 0.5rem 0.25rem;
    }
  `,
  imports: [
    NgTemplateOutlet,
    StoreIconDirective,
    RouterLink,
    TuiButton,
    TuiDropdown,
    TuiDataList,
    TuiIcon,
    TuiTitle,
    TuiAvatar,
    TuiFade,
    i18nPipe,
  ],
})
export class MarketplaceRegistrySelectComponent {
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)
  private readonly marketplace = inject(AbstractMarketplaceService)
  private readonly router = inject(Router)

  public readonly manageRegistriesLink = input('')

  protected open = false
  protected readonly breakpoint = inject(TUI_BREAKPOINT)

  protected readonly fetched = toSignal(this.marketplace.currentRegistry$)

  protected readonly data = toSignal(
    combineLatest([
      this.marketplace.registries$,
      this.marketplace.currentRegistryUrl$,
    ]).pipe(
      map(([registries, currentUrl]) => {
        const withSelected = registries.map(s => ({
          ...s,
          selected: sameUrl(s.url, currentUrl),
        }))

        const current = withSelected.find(s => s.selected)
        const custom = withSelected.filter(s => !isDefaultRegistry(s.url))

        return {
          currentUrl,
          current,
          standard: withSelected.filter(s => isDefaultRegistry(s.url)),
          custom: current
            ? custom
            : [
                ...(this.manageRegistriesLink() ? custom : []),
                { url: currentUrl, name: currentUrl, selected: true },
              ],
        }
      }),
    ),
  )

  async connect(url: string): Promise<void> {
    this.open = false

    if (sameUrl(url, this.data()?.currentUrl)) return

    this.tasks.run(async () => {
      await this.marketplace.connect(url)
      this.router.navigate([], {
        queryParams: { registry: url },
        queryParamsHandling: 'merge',
      })
    }, 'Changing registry')
  }

  async viewCustom(initialValue = ''): Promise<void> {
    this.open = false

    const rawUrl = await firstValueFrom(
      this.dialog
        .openPrompt<string>({
          label: 'View custom registry',
          data: {
            message: 'The domain or URL of the custom registry',
            label: 'URL',
            placeholder: 'e.g. registry.example.com',
            buttonText: 'View',
            initialValue,
          },
        })
        .pipe(defaultIfEmpty('')),
    )

    if (rawUrl) this.connect(registryUrl(rawUrl))
  }
}
