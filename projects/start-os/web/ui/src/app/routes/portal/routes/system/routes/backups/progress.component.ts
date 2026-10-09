import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nPipe, LeafProgressPipe } from '@start9labs/shared'
import { TuiCell, TuiIcon, TuiLoader, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { take } from 'rxjs'

import { ToManifestPipe } from 'src/app/routes/portal/pipes/to-manifest'
import { InstallingProgressPipe } from 'src/app/routes/portal/routes/services/pipes/install-progress.pipe'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Component({
  selector: '[backupProgress]',
  template: `
    @let overallLeaf = backupProgress()?.overall || null | leafProgress;
    @let overallPct = overallLeaf | installingProgress;
    <header>
      <span>{{ 'Backup progress' | i18n }}</span>
      <span class="progress-status">
        @if (overallLeaf === true) {
          {{ 'complete' | i18n }}
        } @else {
          <tui-loader class="overall-loader" size="s" />
          <span>{{ overallPct }}%</span>
        }
      </span>
    </header>
    @for (phase of backupProgress()?.phases; track phase.name) {
      @let pkg = pkgs()?.[phase.name];
      @let leaf = phase.progress | leafProgress;
      @let percent = leaf | installingProgress;
      <div tuiCell class="progress-row">
        <span tuiAvatar appearance="action-grayscale" [round]="false">
          @if (pkg) {
            <img alt="" [src]="pkg.icon" />
          } @else {
            <img alt="StartOS" src="assets/img/icon.png" />
          }
        </span>
        <span tuiTitle>
          <span tuiFade>
            {{ pkg ? (pkg | toManifest).title : ($any(phase.name) | i18n) }}
          </span>
        </span>
        <span class="phase-status">
          @if (leaf === true) {
            <tui-icon icon="@tui.check" class="g-positive" />
            {{ 'complete' | i18n }}
          } @else if (leaf === null) {
            <tui-icon icon="@tui.clock" />
            {{ 'waiting' | i18n }}
          } @else if (leaf === false) {
            <tui-loader size="s" />
            {{ 'backing up' | i18n }}
          } @else {
            <tui-loader size="s" />
            {{ percent }}%
          }
        </span>
      </div>
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 0.5rem;
      inline-size: 100%;
    }

    header {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      justify-content: space-between;
    }

    .progress-status {
      display: flex;
      flex-shrink: 0;
      align-items: center;
      gap: 0.5rem;
      margin-inline-start: auto;
      margin-inline-end: 1rem;
    }

    .overall-loader,
    .phase-status tui-loader {
      color: var(--tui-text-action);
    }

    tui-icon {
      font-size: 1rem;
    }

    [tuiTitle] {
      grid-area: title;
      min-inline-size: 0;
      white-space: nowrap;
    }

    [tuiFade] {
      margin-inline-end: auto;
    }

    .progress-row {
      display: grid;
      grid-template-areas:
        'icon title'
        'icon status';
      grid-template-columns: auto minmax(0, 1fr);
      column-gap: 0.75rem;
      row-gap: 0.125rem;
      align-items: center;
      min-inline-size: 0;
    }

    .progress-row [tuiAvatar] {
      grid-area: icon;
    }

    .phase-status {
      grid-area: status;
      display: inline-flex;
      align-items: center;
      justify-content: flex-end;
      gap: 0.25rem;
      justify-self: end;
      min-inline-size: 4.75rem;
      max-inline-size: 100%;
      text-align: end;
      white-space: nowrap;
    }
  `,
  imports: [
    TuiFade,
    TuiCell,
    TuiAvatar,
    TuiTitle,
    TuiIcon,
    TuiLoader,
    ToManifestPipe,
    LeafProgressPipe,
    InstallingProgressPipe,
    i18nPipe,
  ],
})
export class BackupProgressComponent {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly pkgs = toSignal(
    this.patch.watch$('packageData').pipe(take(1)),
  )
  protected readonly backupProgress = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'backupProgress'),
  )
}
