import { CommonModule, TitleCasePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import {
  i18nPipe,
  MarkdownPipe,
  SafeLinksDirective,
  TaskService,
} from '@start9labs/shared'
import { Version } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Component({
  template: `
    <h2 style="margin-top: 0">{{ 'Release notes' | i18n | titlecase }}</h2>
    <tui-scrollbar style="margin-bottom: 24px; max-height: 50vh;">
      @for (v of versions; track $index) {
        <h4 class="version-header">{{ v.version }}</h4>
        <div safeLinks [innerHTML]="v.notes | markdown | dompurify"></div>
      }
    </tui-scrollbar>
    <button tuiButton style="float: right;" (click)="update()">
      {{ 'Begin Update' | i18n }}
    </button>
  `,
  styles: `
    .version-header {
      font-weight: bold;
      font-size: 1.4rem;
      margin: 2rem 0 0 0;
      color: var(--tui-text-secondary);
    }
  `,
  imports: [
    CommonModule,
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiButton,
    TuiScrollbar,
    i18nPipe,
    TitleCasePipe,
  ],
})
export class SystemUpdateModal {
  private readonly tasks = inject(TaskService)
  private readonly embassyApi = inject(ApiService)
  private readonly os = inject(OSService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly context =
    injectContext<TuiDialogContext<void, { currentVersion: string }>>()

  readonly versions = Object.entries(this.os.osUpdate!)
    .filter(
      ([version]) =>
        Version.parse(version).compare(
          Version.parse(this.context.data.currentVersion),
        ) === 'greater',
    )
    .sort(([a], [b]) => Version.parse(b).compareForSort(Version.parse(a)))
    .map(([version, info]) => ({ version, notes: info.releaseNotes }))

  async update() {
    const { startosRegistry } = await firstValueFrom(this.patch.watch$('ui'))

    this.tasks.run(async () => {
      await this.embassyApi.updateServer({
        targetVersion: `=${this.versions[0]!.version}`,
        registry: startosRegistry,
        progress: false,
      })
      this.context.$implicit.complete()
    }, 'Beginning update')
  }
}

export const UPDATE = new PolymorpheusComponent(SystemUpdateModal)
