import { TuiIcon } from '@taiga-ui/core'
import { DatePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { TuiSkeleton } from '@taiga-ui/kit'
import { CronJob } from 'cron'
import { PatchDB } from 'patch-db-client'
import { from, map } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { GetBackupIconPipe } from '../pipes/get-backup-icon.pipe'

@Component({
  selector: 'table[backupsUpcoming]',
  template: `
    <thead>
      <tr>
        <th>Scheduled</th>
        <th>Job</th>
        <th>Target</th>
        <th>Packages</th>
      </tr>
    </thead>
    @if (current(); as current) {
      <tbody>
        @for (job of upcoming(); track $index) {
          <tr>
            <td class="date">
              @if (current.id === job.id) {
                <span [style.color]="'var(--tui-text-positive)'">Running</span>
              } @else {
                {{ job.next | date: 'medium' }}
              }
            </td>
            <td class="name">{{ job.name }}</td>
            <td>
              @if (targets()?.saved?.[job.targetId]; as target) {
                <tui-icon [icon]="target.type | getBackupIcon" />
                {{ target.name }}
              }
            </td>
            <td class="packages">Packages: {{ job.packageIds.length }}</td>
          </tr>
        } @empty {
          @if (upcoming()) {
            <tr>
              <td colspan="5">You have no active or upcoming backup jobs</td>
            </tr>
          } @else {
            @for (row of ['', '']; track $index) {
              <tr>
                <td colspan="5"><div [tuiSkeleton]="true">Loading</div></td>
              </tr>
            }
          }
        }
      </tbody>
    }
  `,
  styles: `
    :host {
      grid-template-columns: 1fr 1fr;
    }

    .date {
      order: 1;
      grid-column: span 2;
    }

    .name {
      grid-column: span 2;
    }

    tui-icon {
      font-size: 1rem;
      vertical-align: sub;
      margin-inline-end: 0.25rem;
    }

    :host-context(tui-root._mobile) {
      .date {
        color: var(--tui-text-secondary);
      }

      .name {
        text-transform: uppercase;
        font-weight: bold;
      }

      .packages {
        text-align: right;
      }
    }
  `,
  imports: [GetBackupIconPipe, DatePipe, TuiIcon, TuiSkeleton],
})
export class BackupsUpcomingComponent {
  private readonly api = inject(ApiService)

  readonly targets = toSignal(from(this.api.rpc.backup.target.list({})))
  readonly current = toSignal(
    inject<PatchDB<DataModel>>(PatchDB)
      // @TODO 041 remove "as any" once this feature is real
      .watch$('serverInfo', 'statusInfo', 'currentBackup' as any, 'job')
      .pipe(map(job => job || {})),
  )

  readonly upcoming = toSignal(
    from(this.api.getBackupJobs({})).pipe(
      map(jobs =>
        jobs
          .map(job => {
            const nextDate = new CronJob(job.cron, () => {}).nextDate()

            return {
              ...job,
              next: nextDate.toISO(),
              diff: nextDate.diffNow().milliseconds,
            }
          })
          .sort((a, b) => a.diff - b.diff),
      ),
    ),
  )
}
