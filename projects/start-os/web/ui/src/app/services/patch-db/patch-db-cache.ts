import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { Dump } from 'patch-db-client'
import { DataModel } from './data-model'

const VALUES = 'values'
const META = 'meta'
const REVISION = 'revision'

@Service()
export class PatchDbCache {
  private readonly win = inject(WA_WINDOW)
  private db?: Promise<IDBDatabase>
  private saved: Partial<DataModel> = {}

  async read(): Promise<Dump<DataModel>> {
    try {
      const { keys, values, id } = await this.transaction('readonly', tx => ({
        keys: tx.objectStore(VALUES).getAllKeys(),
        values: tx.objectStore(VALUES).getAll(),
        id: tx.objectStore(META).get(REVISION),
      }))
      if (!id.result) return { id: 0, value: {} as DataModel }
      this.saved = Object.fromEntries(
        keys.result.map((key, i) => [key, values.result[i]]),
      )
      return { id: id.result, value: { ...this.saved } as DataModel }
    } catch (e) {
      console.error(e)
      return { id: 0, value: {} as DataModel }
    }
  }

  /** Skips top-level values that are the same objects as the last write's. */
  async write({ id, value }: Dump<DataModel>): Promise<void> {
    const previous = this.saved
    this.saved = { ...value }
    try {
      await this.transaction('readwrite', tx => {
        const store = tx.objectStore(VALUES)
        Object.entries(value).forEach(([key, v]) => {
          if (previous[key as keyof DataModel] !== v) store.put(v, key)
        })
        Object.keys(previous).forEach(key => {
          if (!(key in value)) store.delete(key)
        })
        tx.objectStore(META).put(id, REVISION)
      })
    } catch (e) {
      console.error(e)
      this.saved = {}
      await this.transaction('readwrite', tx =>
        tx.objectStore(META).delete(REVISION),
      ).catch(console.error)
    }
  }

  async clear(): Promise<void> {
    this.saved = {}
    await this.transaction('readwrite', tx => {
      tx.objectStore(VALUES).clear()
      tx.objectStore(META).clear()
    }).catch(console.error)
  }

  private async transaction<T>(
    mode: IDBTransactionMode,
    body: (tx: IDBTransaction) => T,
  ): Promise<T> {
    const db = await (this.db ??= open(this.win))
    return new Promise((resolve, reject) => {
      const tx = db.transaction([VALUES, META], mode)
      const result = body(tx)
      tx.oncomplete = () => resolve(result)
      tx.onerror = tx.onabort = () => reject(tx.error)
    })
  }
}

function open(win: Window): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = win.indexedDB.open('startos-patchdb', 1)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(VALUES)
      request.result.createObjectStore(META)
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}
