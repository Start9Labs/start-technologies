import { Component, computed, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { switchMap } from 'rxjs'

import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getCurrentManifest } from 'src/app/utils/get-package-data'
import { VolumeGrant } from '../../../system/routes/volumes/grant'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle>{{ 'Volumes' | i18n }}</h2>
    </header>
    @if (!enabled()) {
      <p tuiNotification appearance="info">
        {{ 'This service cannot make use of volumes' | i18n }}
      </p>
    }
    <table [appTable]="['Name', 'Access']">
      @for (entry of entries(volumes() || {}); track entry[0]) {
        <tr>
          <td>{{ entry[1].name }}</td>
          <td>
            <volume-grant
              [volumeId]="entry[0]"
              [packageId]="pkgId()"
              [label]="entry[1].name"
              [grant]="entry[1].grants[pkgId()]"
              [disabled]="!enabled()"
            />
          </td>
        </tr>
      } @empty {
        <tr>
          <td colspan="2">
            <p>{{ 'No volumes' | i18n }}</p>
            <a tuiButton size="s" routerLink="/system/volumes">
              {{ 'Manage volumes' | i18n }}
            </a>
          </td>
        </tr>
      }
    </table>
  `,
  host: { class: 'g-subpage' },
  imports: [
    RouterLink,
    i18nPipe,
    TuiButton,
    TuiNotification,
    TuiTitle,
    TuiHeader,
    TableComponent,
    VolumeGrant,
  ],
})
export default class ServiceVolumes {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  public readonly pkgId = input.required<string>()

  protected readonly entries = Object.entries
  protected readonly volumes = toSignal(this.patch.watch$('sharedVolumes'))
  protected readonly pkg = toSignal(
    toObservable(this.pkgId).pipe(
      switchMap(id => this.patch.watch$('packageData', id)),
    ),
  )
  protected readonly enabled = computed(
    (pkg = this.pkg()) => !!pkg && getCurrentManifest(pkg).sharedVolumes,
  )
}
