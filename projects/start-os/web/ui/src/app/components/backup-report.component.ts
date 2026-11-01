import { CommonModule } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import { TuiDialogContext, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { T } from '@start9labs/start-core'
import { getManifest } from '../utils/get-package-data'
import { DataModel } from '../services/patch-db/data-model'

@Component({
  template: `
    <p class="timestamp">{{ data.createdAt | date: 'medium' }}</p>
    <table class="g-table">
      <thead>
        <tr>
          <th>{{ 'Title' | i18n }}</th>
          <th>{{ 'Result' | i18n }}</th>
          <th>{{ 'Duration' | i18n }}</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td>{{ 'System data' | i18n }}</td>
          <td [style.color]="system().color">
            <tui-icon [icon]="system().icon" />
            {{ system().result | i18n
            }}{{
              data.content.server.error ? ': ' + data.content.server.error : ''
            }}
          </td>
          <td></td>
        </tr>
        @if (pkgTitles(); as titles) {
          @for (
            item of data.content | tuiMapper: reportRows : titles;
            track item.key
          ) {
            <tr>
              <td>
                <span tuiTitle>
                  <b>{{ item.title }}</b>
                  @if (item.kind === 'volume') {
                    <span tuiSubtitle>{{ 'Volumes' | i18n }}</span>
                  }
                </span>
              </td>
              <td [style.color]="getColor(item.result.error)">
                <tui-icon [icon]="getIcon(item.result.error)" />
                {{
                  item.result.error
                    ? ('Failed' | i18n) + ': ' + item.result.error
                    : ('Succeeded' | i18n)
                }}
              </td>
              <td>{{ formatDuration(item.result.duration_ms) }}</td>
            </tr>
          }
        }
      </tbody>
    </table>
  `,
  styles: `
    .timestamp {
      color: var(--tui-text-secondary);
      margin: 0 0 1rem;
    }

    td:first-child {
      white-space: nowrap;
    }

    tui-icon {
      font-size: 1rem;
      vertical-align: sub;
      margin-inline-end: 0.25rem;
    }
  `,
  imports: [CommonModule, TuiIcon, TuiTitle, TuiMapperPipe, i18nPipe],
})
export class BackupsReportModal {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly data =
    injectContext<
      TuiDialogContext<void, { content: T.BackupReport; createdAt: string }>
    >().data

  readonly pkgTitles = toSignal(
    this.patch.watch$('packageData').pipe(
      map(allPkgs =>
        Object.values(allPkgs).reduce<Record<string, string>>((acc, pkg) => {
          const { id, title } = getManifest(pkg)
          return {
            ...acc,
            [id]: title,
          }
        }, {}),
      ),
    ),
  )

  protected readonly reportRows = (
    content: T.BackupReport,
    titles: Record<string, string>,
  ) => [
    ...Object.entries(content.packages).map(([id, result]) => ({
      key: `package:${id}`,
      kind: 'package' as const,
      title: titles[id] || id,
      result,
    })),
    ...Object.entries(content.sharedVolumes ?? {}).map(([id, result]) => ({
      key: `volume:${id}`,
      kind: 'volume' as const,
      title: result.name,
      result,
    })),
  ]

  readonly system = computed(
    (): { result: i18nKey; icon: string; color: string } => {
      if (!this.data.content.server.attempted) {
        return {
          result: 'Not attempted',
          icon: '@tui.minus',
          color: 'var(--tui-text-secondary)',
        }
      }

      if (this.data.content.server.error) {
        return {
          result: 'Failed',
          icon: '@tui.circle-minus',
          color: 'var(--tui-text-negative)',
        }
      }

      return {
        result: 'Succeeded',
        icon: '@tui.check',
        color: 'var(--tui-text-positive)',
      }
    },
  )

  getColor(error: unknown) {
    return error ? 'var(--tui-text-negative)' : 'var(--tui-text-positive)'
  }

  getIcon(error: unknown) {
    return error ? '@tui.circle-minus' : '@tui.check'
  }

  formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`
    const seconds = Math.round(ms / 1000)
    if (seconds < 60) return `${seconds}s`
    const minutes = Math.floor(seconds / 60)
    const remainder = seconds % 60
    return remainder ? `${minutes}m ${remainder}s` : `${minutes}m`
  }
}

export const REPORT = new PolymorpheusComponent(BackupsReportModal)
