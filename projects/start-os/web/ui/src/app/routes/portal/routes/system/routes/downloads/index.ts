import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule, NgModel } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { DOWNLOADS_MENU_ITEM } from '../../system.const'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ entry.item | i18n }}
      </div>
    </ng-container>
    <section tuiCardLarge>
      <header tuiHeader="body-l">
        <h2 tuiTitle>{{ entry.item | i18n }}</h2>
        <a
          tuiIconButton
          docsLink
          path="/start-os/marketplace.html"
          fragment="#optional-seeding"
          size="xs"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
      </header>
      @if (availability() === 'error') {
        <div tuiNotification appearance="negative">
          {{ 'Download settings could not be loaded.' | i18n }}
          <button tuiButton appearance="flat" size="s" (click)="load()">
            {{ 'Retry' | i18n }}
          </button>
        </div>
      } @else if (availability() === 'loading' || seeding() === undefined) {
        <tui-loader />
      } @else {
        @if (!availability()) {
          <div tuiNotification appearance="info">
            {{
              'BitTorrent is disabled in the server configuration. Your sharing preference applies when BitTorrent is enabled.'
                | i18n
            }}
          </div>
        }
        <label tuiCell>
          <span tuiTitle>
            <b>{{ 'Continue seeding' | i18n }}</b>
            <span tuiSubtitle>
              {{
                'Retain verified BitTorrent downloads to share after installation and updates.'
                  | i18n
              }}
            </span>
          </span>
          <input
            #control="ngModel"
            tuiSwitch
            type="checkbox"
            [disabled]="saving()"
            [ngModel]="seeding()"
            (ngModelChange)="save($event, control)"
          />
        </label>
        <p tuiSubtitle>
          {{
            'Active BitTorrent downloads can already upload. This setting controls sharing after downloads finish.'
              | i18n
          }}
        </p>
      }
      <div tuiNotification appearance="warning">
        {{
          "Peers can see your server's IP address and the torrents it shares."
            | i18n
        }}
      </div>
      <p tuiSubtitle>
        {{
          'Retained copies use additional disk space and remain until removed. Disabling continued seeding keeps those copies.'
            | i18n
        }}
      </p>
    </section>
  `,
  imports: [
    FormsModule,
    RouterLink,
    DocsLinkDirective,
    i18nPipe,
    TuiButton,
    TuiCell,
    TuiLoader,
    TuiNotification,
    TuiTitle,
    TuiSwitch,
    TuiCardLarge,
    TuiHeader,
    TitleDirective,
  ],
})
export default class Downloads {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly entry = DOWNLOADS_MENU_ITEM
  protected readonly seeding = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('serverInfo', 'registrySeeding'),
  )
  protected readonly availability = signal<boolean | 'loading' | 'error'>(
    'loading',
  )
  protected readonly saving = signal(false)

  constructor() {
    this.load()
  }

  protected async load() {
    this.availability.set('loading')
    if (
      !(await this.tasks.runInline(async () => {
        this.availability.set(await this.api.rpc.server.bittorrent.enabled({}))
      }))
    )
      this.availability.set('error')
  }

  protected async save(enabled: boolean, control: NgModel) {
    this.saving.set(true)
    const success = await this.tasks.run(
      () => this.api.rpc.server.bittorrent.setSeeding({ enabled }),
      'Saving',
    )
    if (!success)
      control.control.setValue(this.seeding(), { emitViewToModelChange: false })
    this.saving.set(false)
  }
}
