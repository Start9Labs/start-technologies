import { Component, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router, RouterLink } from '@angular/router'
import {
  DialogService,
  i18nPipe,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import {
  TUI_BREAKPOINT,
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import {
  combineLatest,
  defaultIfEmpty,
  filter,
  firstValueFrom,
  map,
} from 'rxjs'

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
          @if (!manageRegistriesLink() && d.unsaved; as url) {
            <button tuiOption (click)="saveCurrent(url)">
              <span tuiAvatar><img [storeIcon]="url" /></span>
              <span tuiTitle>
                {{ fetched()?.info?.name || url }}
                <span tuiSubtitle>{{ 'Save to this browser' | i18n }}</span>
              </span>
              <tui-icon icon="@tui.bookmark" />
            </button>
            <hr />
          }
          <div class="g-title">
            <small>{{ 'Saved Registries' | i18n }}</small>
            @if (manageRegistriesLink()) {
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
              <button
                tuiButton
                size="xs"
                appearance="primary"
                iconStart="@tui.plus"
                (click)="add()"
              >
                {{ 'Add' | i18n }}
              </button>
            }
          </div>
          @for (
            registry of manageRegistriesLink() ? d.registries : d.standard;
            track registry.url
          ) {
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
          }
          @for (
            registry of manageRegistriesLink() ? [] : d.custom;
            track registry.url
          ) {
            @if ($first) {
              <hr />
            }
            <div class="custom">
              <button
                tuiOption
                (click)="connect(registry.url)"
                (keydown.backspace)="delete(registry.url)"
                (keydown.delete)="delete(registry.url)"
              >
                <span tuiAvatar><img [storeIcon]="registry.url" /></span>
                <span tuiTitle>
                  {{ registry.name }}
                  <span tuiSubtitle>{{ registry.url }}</span>
                </span>
                @if (registry.selected) {
                  <tui-icon icon="@tui.check" />
                }
              </button>
              <button
                tuiIconButton
                size="xs"
                appearance="icon"
                iconStart="@tui.trash"
                [tuiHint]="'Remove' | i18n"
                (click)="delete(registry.url)"
              >
                {{ 'Remove' | i18n }}
              </button>
            </div>
          }
        </tui-data-list>
      }
    </button>
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

    .custom {
      display: flex;
      align-items: center;
      gap: 0.25rem;

      [tuiOption] {
        flex: 1;
        min-inline-size: 0;
      }
    }
  `,
  imports: [
    StoreIconDirective,
    RouterLink,
    TuiButton,
    TuiDropdown,
    TuiDataList,
    TuiHint,
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

        return {
          currentUrl,
          current,
          registries: withSelected,
          standard: withSelected.slice(0, 2),
          custom: withSelected.slice(2),
          unsaved: current ? null : currentUrl,
        }
      }),
    ),
  )

  async connect(url: string): Promise<void> {
    this.open = false

    if (sameUrl(url, this.data()?.current?.url)) return

    this.tasks.run(async () => {
      await this.marketplace.connect(url)
      this.router.navigate([], {
        queryParams: { registry: url },
        queryParamsHandling: 'merge',
      })
    }, 'Changing registry')
  }

  async add(): Promise<void> {
    this.open = false

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

  async saveCurrent(url: string): Promise<void> {
    this.open = false
    this.tasks.run(async () => await this.marketplace.add(url), 'Saving')
  }

  delete(url: string): void {
    this.dialog
      .openConfirm({
        label: 'Remove registry?',
        size: 's',
        data: {
          content:
            'This only removes the registry from the list saved in this browser. The registry itself is not affected.',
          yes: 'Remove',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () => await this.marketplace.delete(url),
          'Removing',
        ),
      )
  }
}
