import { Component, computed, inject, linkedSignal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { TUI_BREAKPOINT, TuiButton, TuiIcon } from '@taiga-ui/core'
import { TuiSegmented } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { map, shareReplay, take } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel, ServicesView } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { PlaceholderComponent } from '../../../components/placeholder.component'
import { ServicesGridComponent } from './grid.component'
import { ServicesTableComponent } from './table.component'

const DEFAULT: ServicesView = { desktopLayout: 'list', asc: true }

@Component({
  template: `
    <ng-container *title>{{ 'Installed services' | i18n }}</ng-container>

    @if (services()?.length === 0) {
      <app-placeholder>
        <h1 [style.margin-bottom]="0">
          {{ 'Welcome to' | i18n }}
          <span>StartOS!</span>
        </h1>

        <p>
          {{
            'To get started, visit the Marketplace and download your first service'
              | i18n
          }}
        </p>

        <a
          style="margin: 1.5rem 0;"
          tuiButton
          size="m"
          iconStart="@tui.shopping-cart"
          routerLink="../marketplace"
        >
          {{ 'View Marketplace' | i18n }}
        </a>
      </app-placeholder>
    } @else {
      <header>
        @if (grid()) {
          <button
            appearance="secondary-grayscale"
            size="s"
            tuiIconButton
            [iconStart]="
              view().asc
                ? '@tui.arrow-up-narrow-wide'
                : '@tui.arrow-down-wide-narrow'
            "
            (click)="update({ asc: !view().asc })"
          >
            {{ (view().asc ? 'Ascending' : 'Descending') | i18n }}
          </button>
        }
        @if (!mobile()) {
          <tui-segmented size="s" [activeItemIndex]="grid() ? 1 : 0">
            <button
              type="button"
              [attr.aria-label]="'List view' | i18n"
              [title]="'List view' | i18n"
              (click)="update({ desktopLayout: 'list' })"
            >
              <tui-icon icon="@tui.list" />
            </button>
            <button
              type="button"
              [attr.aria-label]="'Grid view' | i18n"
              [title]="'Grid view' | i18n"
              (click)="update({ desktopLayout: 'grid' })"
            >
              <tui-icon icon="@tui.layout-grid" />
            </button>
          </tui-segmented>
        }
      </header>

      @if (grid()) {
        <div [asc]="view().asc" [servicesGrid]="services()"></div>
      } @else {
        <div [services]="services()"></div>
      }
    }
  `,
  styles: `
    :host {
      padding: 1rem;
    }

    header {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 0.5rem;
      margin-block-end: 1rem;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    PlaceholderComponent,
    RouterLink,
    ServicesGridComponent,
    ServicesTableComponent,
    TitleDirective,
    TuiButton,
    TuiIcon,
    TuiSegmented,
    i18nPipe,
  ],
})
export default class DashboardComponent {
  private readonly api = inject(ApiService)
  private readonly breakpoint = inject(TUI_BREAKPOINT)
  private readonly errorService = inject(ErrorService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  // Seeded once: a later echo of our own write would clobber a newer local change.
  private readonly persisted = toSignal(
    this.patch.watch$('ui', 'servicesView').pipe(take(1)),
  )
  private write: Promise<unknown> = Promise.resolve()

  protected readonly services = toSignal(
    this.patch.watch$('packageData').pipe(
      map(pkgs => Object.values(pkgs)),
      shareReplay(1),
    ),
    { initialValue: null },
  )

  protected readonly mobile = computed(() => this.breakpoint() === 'mobile')
  protected readonly view = linkedSignal(() => this.persisted() || DEFAULT)
  protected readonly grid = computed(
    () => this.mobile() || this.view().desktopLayout === 'grid',
  )

  protected update(patch: Partial<ServicesView>) {
    const next = { ...this.view(), ...patch }

    this.view.set(next)
    // Chained so rapid clicks reach the server in click order and the last wins.
    this.write = this.write
      .then(() => this.api.setDbValue<ServicesView>(['servicesView'], next))
      .catch(e => this.errorService.handleError(e))
  }
}
