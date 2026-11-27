import { inject, Injectable, InjectionToken } from '@angular/core'
import {
  Cursor,
  Dump,
  Selection,
  SubtreeEvent,
  SubtreePatchDB,
  SubtreeSource,
} from 'patch-db-client'
import { Observable } from 'rxjs'
import { BehaviorSubject } from 'rxjs'
import { ApiService } from '../api/embassy-api.service'
import { DataModel } from './data-model'

export const PATCH_CACHE = new InjectionToken('', {
  factory: () =>
    new BehaviorSubject<Dump<DataModel>>({
      id: 0,
      value: {} as DataModel,
    }),
})

/** True while every path the app always watches is observed live. */
export const PATCH_LIVE = new InjectionToken('', {
  factory: () => new BehaviorSubject(false),
})

/** Paths watched for the whole session; the dashboard is live once they are. */
export const ALWAYS_WATCHED: readonly (keyof DataModel)[][] = [
  ['serverInfo'],
  ['packageData'],
  ['ui'],
]

@Injectable({
  providedIn: 'root',
})
export class PatchDbSource implements SubtreeSource {
  private readonly api = inject(ApiService)

  subscribe(
    path: string[],
    since: Cursor | null,
    selection: Selection,
  ): Observable<SubtreeEvent> {
    return this.api.watchDb$(path, since, selection)
  }

  update(subscription: string, selection: Selection): Promise<void> {
    return this.api.updateDbSubscription(subscription, selection)
  }
}

export const PATCH_DB = new InjectionToken('', {
  factory: () =>
    new SubtreePatchDB<DataModel>(inject(PatchDbSource), inject(PATCH_CACHE)),
})
