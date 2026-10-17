import { inject, Injectable } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { combineLatest, EMPTY, map, Observable, shareReplay } from 'rxjs'

import { HiddenUpdatesService } from 'src/app/services/hidden-updates.service'
import { LocalPackagesService } from 'src/app/services/local-packages.service'
import { NotificationService } from 'src/app/services/notification.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { UpdatesRefinementService } from 'src/app/services/updates-refinement.service'
import { FilterUpdatesPipe } from '../routes/portal/routes/updates/filter-updates.pipe'

@Injectable({
  providedIn: 'root',
})
export class BadgeService {
  private readonly notifications = inject(NotificationService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly general$ = inject(OSService).updateAvailable$.pipe(
    map(Number),
  )
  private readonly metrics$ = this.patch
    .watch$('serverInfo', 'ntpSynced')
    .pipe(map(synced => Number(!synced)))
  private readonly backups$ = this.patch
    .watch$('scheduledBackups', 'activities')
    .pipe(
      map(
        activities =>
          Object.values(activities).filter(
            activity => activity.state === 'running',
          ).length,
      ),
    )
  private readonly system$ = this.general$
  private readonly filterUpdatesPipe = inject(FilterUpdatesPipe)
  private readonly hiddenUpdates = inject(HiddenUpdatesService)
  private readonly refinement = inject(UpdatesRefinementService)

  // Counts over the *refined* marketplace so the badge reflects the same
  // reachable-updates set the Updates tab displays. Subscribing here also
  // keeps the refinement pipeline warm for the lifetime of the navbar, which
  // is what lets the Updates tab replay the already-resolved state on mount
  // instead of running refinement again on each navigation.
  private readonly updates$ = combineLatest([
    this.refinement.refined$,
    inject(LocalPackagesService),
    this.hiddenUpdates.effective$,
  ]).pipe(
    map(
      ([refined, local, hidden]) =>
        Object.entries(refined.marketplace).reduce(
          (list, [_, store]) =>
            this.filterUpdatesPipe
              .transform(store?.packages || [], local, hidden)
              .reduce((result, { id }) => result.add(id), list),
          new Set<string>(),
        ).size,
    ),
    shareReplay(1),
  )

  getCount(id: string): Observable<number> {
    switch (id) {
      case 'updates':
        return this.updates$
      case 'system':
        return this.system$
      case 'general':
        return this.general$
      case 'metrics':
        return this.metrics$
      case 'backups':
        return this.backups$
      case 'notifications':
        return this.notifications.unreadCount$
      default:
        return EMPTY
    }
  }
}
