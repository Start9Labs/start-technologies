import { KeyValuePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nPipe } from '@start9labs/shared'
import { TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'

import { DataModel } from 'src/app/services/patch-db/data-model'
import { getCurrentManifest } from 'src/app/utils/get-package-data'
import { VolumeGrant } from './grant'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Service access' | i18n }}</h2>
        <p tuiSubtitle>{{ volumes()?.[context.data.id]?.name }}</p>
      </hgroup>
    </header>
    @for (entry of packages() | keyvalue: null; track entry.key) {
      @let manifest = getCurrentManifest(entry.value);
      <div tuiCell>
        <span tuiTitle>
          {{ manifest.title }}
          @if (!manifest.sharedVolumes) {
            <span tuiSubtitle>
              {{ 'This service cannot make use of volumes' | i18n }}
            </span>
          }
        </span>
        <volume-grant
          [volumeId]="context.data.id"
          [packageId]="entry.key"
          [label]="manifest.title"
          [grant]="volumes()?.[context.data.id]?.grants?.[entry.key]"
          [disabled]="!manifest.sharedVolumes"
        />
      </div>
    } @empty {
      <p>{{ 'No services installed' | i18n }}</p>
    }
  `,
  imports: [i18nPipe, TuiCell, TuiTitle, TuiHeader, KeyValuePipe, VolumeGrant],
})
class VolumeGrants {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly context =
    injectContext<TuiDialogContext<void, { id: string }>>()
  protected readonly volumes = toSignal(this.patch.watch$('sharedVolumes'))
  protected readonly packages = toSignal(this.patch.watch$('packageData'), {
    initialValue: {},
  })
  protected readonly getCurrentManifest = getCurrentManifest
}

export const VOLUME_GRANTS = new PolymorpheusComponent(VolumeGrants)
