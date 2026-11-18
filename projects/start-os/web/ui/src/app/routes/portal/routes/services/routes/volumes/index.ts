import { KeyValuePipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiHint,
  TuiIcon,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { switchMap } from 'rxjs'

import { DataModel } from 'src/app/services/patch-db/data-model'
import { getCurrentManifest } from 'src/app/utils/get-package-data'
import { VolumeGrant } from '../../../system/routes/volumes/grant'
import {
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from '../../../system/routes/volumes/types'

@Component({
  template: `
    <section class="g-card">
      <header>
        {{ 'Shared Volumes' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/shared-volumes.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
      </header>
      @let volumeList = volumes() | keyvalue: null;
      @if (!enabled()) {
        <p tuiNotification appearance="info">
          {{ 'This service cannot use shared volumes' | i18n }}
        </p>
      }
      @for (entry of volumeList; track entry.key) {
        <div tuiCell>
          <span tuiTitle>
            {{ name(entry.value) }}
            <span tuiSubtitle class="location">
              <tui-icon
                [icon]="types[entry.value.source.kind].icon"
                [tuiHint]="types[entry.value.source.kind].label | i18n"
                [tuiHintShowDelay]="128"
              />
              {{ path(entry.value.source) }}
            </span>
          </span>
          <volume-grant
            [volumeId]="entry.key"
            [packageId]="pkgId()"
            [label]="name(entry.value)"
            [grant]="entry.value.grants[pkgId()]"
            [disabled]="!enabled()"
          />
        </div>
      } @empty {
        <p>{{ 'No shared volumes' | i18n }}</p>
        <footer>
          <a tuiButton size="s" routerLink="/system/volumes">
            {{ 'Manage shared volumes' | i18n }}
          </a>
        </footer>
      }
    </section>
  `,
  styles: `
    section {
      max-inline-size: 32rem;
    }

    p {
      margin: 0 0 0.75rem;
    }

    .location {
      display: flex;
      align-items: center;
      gap: 0.375rem;

      tui-icon {
        font-size: 1rem;
      }
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    RouterLink,
    DocsLinkDirective,
    i18nPipe,
    TuiButton,
    TuiCell,
    TuiHint,
    TuiIcon,
    KeyValuePipe,
    TuiNotification,
    TuiTitle,
    VolumeGrant,
  ],
})
export default class ServiceVolumes {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly types = VOLUME_TYPES
  protected readonly name = volumeName
  protected readonly path = volumePath

  public readonly pkgId = input.required<string>()

  protected readonly volumes = toSignal(this.patch.watch$('sharedVolumes'), {
    initialValue: {},
  })
  protected readonly pkg = toSignal(
    toObservable(this.pkgId).pipe(
      switchMap(id => this.patch.watch$('packageData', id)),
    ),
  )
  protected readonly enabled = computed(
    (pkg = this.pkg()) => !!pkg && getCurrentManifest(pkg).sharedVolumes,
  )
}
