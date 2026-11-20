import { inject, Injectable } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { combineLatest, EMPTY, map, Observable, of, shareReplay } from 'rxjs'
import { conversionLabel } from 'src/app/routes/portal/routes/system/routes/backups/conversion-state'
import { HiddenUpdatesService } from 'src/app/services/hidden-updates.service'
import { LocalPackagesService } from 'src/app/services/local-packages.service'
import { NotificationService } from 'src/app/services/notification.service'
import { OSService } from 'src/app/services/os.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import {
  FilterUpdatesPipe,
  HasSecurityUpdatePipe,
} from '../routes/portal/routes/updates/filter-updates.pipe'

/** Counts tally pending items; warnings flag conditions that need attention. */
@Injectable({
  providedIn: 'root',
})
export class BadgeService {
  private readonly notifications = inject(NotificationService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly system$ = inject(OSService).showUpdate$.pipe(map(Number))
  private readonly metrics$ = this.patch
    .watch$('serverInfo', 'ntpSynced')
    .pipe(map(synced => !synced))
  private readonly filterUpdatesPipe = inject(FilterUpdatesPipe)
  private readonly hasSecurityUpdatePipe = inject(HasSecurityUpdatePipe)
  private readonly hiddenUpdates = inject(HiddenUpdatesService)

  private readonly marketplace = inject(MarketplaceService)
  private readonly registryErrors$ = this.marketplace.requestErrors$.pipe(
    map(errors => errors.length > 0),
  )

  private readonly updates$ = combineLatest([
    this.marketplace.marketplace$,
    inject(LocalPackagesService),
    this.hiddenUpdates.effective$,
  ]).pipe(
    map(([marketplace, local, hidden]) => {
      const updates = Object.values(marketplace).flatMap(store =>
        this.filterUpdatesPipe.transform(store?.updates || [], local, hidden),
      )
      return {
        count: new Set(updates.map(({ id }) => id)).size,
        security: this.hasSecurityUpdatePipe.transform(updates, local),
      }
    }),
    shareReplay(1),
  )

  private readonly backups$ = combineLatest([
    this.patch.watch$('backupTargets'),
    this.patch.watch$('serverInfo', 'id'),
    this.patch.watch$('serverInfo', 'statusInfo', 'backupConversions'),
  ]).pipe(
    map(([targets, serverId, jobs]) =>
      Object.entries(targets).some(
        ([id, target]) =>
          target.legacyBackup ||
          !!conversionLabel(target.startOs, jobs[id], serverId),
      ),
    ),
    shareReplay(1),
  )

  getWarning(id: string): Observable<boolean> {
    switch (id) {
      case 'system':
        return combineLatest([this.backups$, this.registryErrors$]).pipe(
          map(([backups, registries]) => backups || registries),
        )
      case 'backup-targets':
        return this.backups$
      case 'registries':
        return this.registryErrors$
      case 'metrics':
        return this.metrics$
      default:
        return of(false)
    }
  }

  getSecurity(id: string): Observable<boolean> {
    return id === 'updates'
      ? this.updates$.pipe(map(({ security }) => security))
      : of(false)
  }

  getCount(id: string): Observable<number> {
    switch (id) {
      case 'updates':
        return this.updates$.pipe(map(({ count }) => count))
      case 'system':
        return this.system$
      case 'notifications':
        return this.notifications.unreadCount$
      default:
        return EMPTY
    }
  }
}
