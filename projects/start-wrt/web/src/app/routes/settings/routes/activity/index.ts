import { DatePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import {
  TUI_TABLE_PAGINATION_TEXTS,
  TuiTable,
  TuiTablePagination,
  tuiTablePaginationOptionsProvider,
} from '@taiga-ui/addon-table'
import { TuiButton, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { Placeholder } from 'src/app/components/placeholder'
import { ActivityEntry, ApiService } from 'src/app/services/api/api.service'
import { ActionService } from 'src/app/services/action.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Activity' | i18n }}</h2>
      @if (activity().length) {
        <button tuiButton size="s" appearance="flat" (click)="clearAll()">
          {{ 'Clear All' | i18n }}
        </button>
      }
    </header>
    <table tuiTable class="g-table">
      <thead>
        <tr>
          <th tuiTh></th>
          <th tuiTh>{{ 'Date' | i18n }}</th>
          <th tuiTh>{{ 'Detail' | i18n }}</th>
          <th tuiTh></th>
        </tr>
      </thead>
      <tbody>
        @for (item of activity(); track item.id) {
          <tr>
            <td tuiTd>
              <tui-icon
                [icon]="item.success ? '@tui.check' : '@tui.x'"
                [class.g-positive]="item.success"
                [class.g-negative]="!item.success"
              />
            </td>
            <td tuiTd>{{ item.timestamp | date: 'medium' }}</td>
            <td tuiTd>
              {{ item.summary }}
              @if (item.error) {
                <span class="error-detail">{{ item.error }}</span>
              }
            </td>
            <td tuiTd>
              <button
                tuiIconButton
                type="button"
                size="xs"
                appearance="icon"
                iconStart="@tui.trash"
                (click)="remove(item)"
              >
                {{ 'Delete' | i18n }}
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td tuiTd colspan="4">
              <app-placeholder icon="@tui.activity">
                {{ 'No activity' | i18n }}
              </app-placeholder>
            </td>
          </tr>
        }
      </tbody>
    </table>
    <tui-table-pagination
      [total]="total()"
      [page]="page()"
      [size]="size()"
      (paginationChange)="onPageChange($event.page, $event.size)"
    />
  `,
  styles: `
    :host {
      max-width: 50rem;
    }

    header {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    td:first-child {
      width: 2rem;
    }

    td:last-child {
      text-align: end;
    }

    .error-detail {
      display: block;
      font-size: 0.75rem;
      color: var(--tui-text-tertiary);
    }

    tui-table-pagination {
      margin-top: 0.5rem;
    }
  `,
  host: { class: 'g-page' },
  providers: [
    tuiTablePaginationOptionsProvider({ showPages: false }),
    {
      provide: TUI_TABLE_PAGINATION_TEXTS,
      useValue: signal({ linesPerPage: '', of: 'of', pages: '' }),
    },
  ],
  imports: [
    DatePipe,
    TuiTable,
    TuiTablePagination,
    TuiButton,
    TuiIcon,
    TuiTitle,
    TuiHeader,
    Placeholder,
    i18nPipe,
  ],
})
export default class Activity {
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly i18n = inject(i18nPipe)

  protected readonly page = signal(0)
  protected readonly size = signal(10)
  protected readonly total = signal(0)
  protected readonly activity = signal<ActivityEntry[]>([])

  constructor() {
    this.load()
  }

  protected async load() {
    const res = await this.api.activityList({
      offset: this.page() * this.size(),
      limit: this.size(),
    })
    this.activity.set(res.entries)
    this.total.set(res.total)
  }

  protected onPageChange(page: number, size: number) {
    this.page.set(page)
    this.size.set(size)
    this.load()
  }

  protected async remove(item: ActivityEntry) {
    await this.actions.run(
      async () => {
        await this.api.activityDelete({ id: item.id })
        if (this.activity().length === 1 && this.page() > 0) {
          this.page.update(p => p - 1)
        }
        await this.load()
      },
      { success: this.i18n.transform('Entry deleted') },
    )
  }

  protected async clearAll() {
    await this.actions.run(
      async () => {
        await this.api.activityClear()
        this.activity.set([])
        this.total.set(0)
        this.page.set(0)
      },
      {
        loading: this.i18n.transform('Clearing activity log...'),
        success: this.i18n.transform('Activity log cleared'),
      },
    )
  }
}
