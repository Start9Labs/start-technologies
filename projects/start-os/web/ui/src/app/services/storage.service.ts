import { inject, Injectable } from '@angular/core'
import { WA_LOCAL_STORAGE } from '@ng-web-apis/common'

const PREFIX = '_startos/'

@Injectable({
  providedIn: 'root',
})
export class StorageService {
  private readonly storage = inject(WA_LOCAL_STORAGE)

  get<T>(key: string): T {
    return JSON.parse(String(this.storage?.getItem(`${PREFIX}${key}`) || null))
  }

  set(key: string, value: any) {
    this.storage?.setItem(`${PREFIX}${key}`, JSON.stringify(value))
  }

  clear() {
    this.storage?.clear()
  }

  migrate() {
    if (this.storage?.getItem('_embassystorage/_embassykv/loggedInKey')) {
      this.clear()
    }
    this.storage?.removeItem(`${PREFIX}patchDB`)
  }
}
