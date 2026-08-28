import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  DialogService,
  i18nPipe,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
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
import { ADD_REGISTRY } from './add-registry.component'
import { StoreIconDirective } from './store-icon.directive'

@Component({
  selector: 'marketplace-registry-select',
  template: `
    <button
      tuiButton
      tuiDropdown
      tuiDropdownSided
      iconEnd="@tui.chevron-right"
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
            <small>{{ 'Saved Registries' | i18n }}</small>
            <button
              tuiButton
              size="xs"
              appearance="primary"
              iconStart="@tui.plus"
              (click)="add()"
            >
              {{ 'Add' | i18n }}
            </button>
          </div>
          @for (registry of d.standard; track registry.url) {
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
          @if (d.unsaved; as url) {
            <hr />
            <button tuiOption (click)="saveCurrent(url)">
              <span tuiAvatar><img [storeIcon]="url" /></span>
              <span tuiTitle>
                {{ fetched()?.info?.name || url }}
                <span tuiSubtitle>{{ 'Save this registry' | i18n }}</span>
              </span>
              <tui-icon icon="@tui.bookmark" />
            </button>
          }
          @for (registry of d.custom; track registry.url) {
            @if ($first) {
              <hr />
            }
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
              <tui-icon icon="@tui.trash" (click.stop)="delete(registry.url)" />
              @if (registry.selected) {
                <tui-icon icon="@tui.check" />
              }
            </button>
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
  `,
  imports: [
    StoreIconDirective,
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

  protected open = false

  // The resolved current registry — used to label an arbitrary (deep-linked)
  // registry that isn't in the saved list yet.
  protected readonly fetched = toSignal(this.marketplace.currentRegistry$)

  // 0 and 1 are the standard registries (start9, community); 2+ are custom.
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
          standard: withSelected.slice(0, 2),
          custom: withSelected.slice(2),
          // an arbitrary (deep-linked) registry not in the saved list
          unsaved: current ? null : currentUrl,
        }
      }),
    ),
  )

  async connect(url: string): Promise<void> {
    this.open = false

    // Already on this registry: just close the dropdown
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

    const saved = await firstValueFrom(this.marketplace.registries$)
    const rawUrl = await firstValueFrom(
      this.dialog
        .openComponent<string>(ADD_REGISTRY, {
          label: 'Add a Registry',
          data: (
            await firstValueFrom(this.marketplace.knownRegistries$)
          ).filter(k => !saved.some(s => sameUrl(s.url, k.url))),
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
