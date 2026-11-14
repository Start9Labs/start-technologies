import { inject, Injectable } from '@angular/core'
import { Dump } from 'patch-db-client'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { StorageService } from '../storage.service'

@Injectable({
  providedIn: 'root',
})
export class LocalStorageBootstrap {
  private readonly storage = inject(StorageService)

  static CONTENT_KEY = 'patchDB'

  init(): Dump<DataModel> {
    const cache = this.storage.get<DataModel>(LocalStorageBootstrap.CONTENT_KEY)

    return cache
      ? {
          id: 1,
          value: {
            ...cache,
            registryCache: cache.registryCache ?? {},
            osUpdateCache: cache.osUpdateCache ?? null,
          },
        }
      : { id: 0, value: {} as DataModel }
  }

  update(cache: DataModel): void {
    this.storage.set(LocalStorageBootstrap.CONTENT_KEY, cache)
  }
}
