import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nPipe, LeafProgressPipe } from '@start9labs/shared'
import { TuiIcon, TuiLoader, TuiTitle, TuiCell } from '@taiga-ui/core'
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
      {{ 'Backup Progress' | i18n }}
      @if (overallLeaf === true) {
        <span>{{ 'complete' | i18n }}</span>
      } @else {
        <span>{{ overallPct }}%</span>
      }
    </header>
    @for (phase of backupProgress()?.phases; track phase.name) {
      @let pkg = pkgs()?.[phase.name];
      @let leaf = phase.progress | leafProgress;
      @let percent = leaf | installingProgress;
      <div tuiCell>
        <span tuiAvatar appearance="action-grayscale" [round]="false">
          @if (pkg) {
            <img alt="" [src]="pkg.icon" />
          } @else {
            <img alt="StartOS" src="assets/img/icon.png" />
          }
        </span>
        <span tuiTitle>
          <span class="title">
            <span tuiFade>
              {{ pkg ? (pkg | toManifest).title : ($any(phase.name) | i18n) }}
            </span>
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
        </span>
      </div>
    }
  `,
  styles: `
    :host {
      max-width: 36rem;
      text-transform: capitalize;
    }

    header {
      justify-content: space-between;
    }

    tui-icon {
      font-size: 1rem;
    }

    [tuiTitle] {
      flex: 1;
      white-space: nowrap;
    }

    [tuiFade] {
      margin-inline-end: auto;
    }

    .title {
      display: flex;
      align-items: center;
      gap: 0.25rem;
    }
  `,
  host: { class: 'g-card' },
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

  readonly pkgs = toSignal(this.patch.watch$('packageData').pipe(take(1)))
  readonly backupProgress = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'backupProgress'),
  )
}
