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
  private saved?: Dump<DataModel>

  async read(): Promise<Dump<DataModel>> {
    try {
      const { keys, values, id } = await this.transaction('readonly', tx => ({
        keys: tx.objectStore(VALUES).getAllKeys(),
        values: tx.objectStore(VALUES).getAll(),
        id: tx.objectStore(META).get(REVISION),
      }))
      if (!id.result) {
        this.saved = undefined
        return { id: 0, value: {} as DataModel }
      }
      const value = Object.fromEntries(
        keys.result.map((key, i) => [key, values.result[i]]),
      ) as DataModel
      this.saved = { id: id.result, value }
      return { id: id.result, value: { ...value } }
    } catch (e) {
      console.error(e)
      this.saved = undefined
      return { id: 0, value: {} as DataModel }
    }
  }

  /** Stored revisions describe complete snapshots across browser tabs. */
  async write({ id, value }: Dump<DataModel>): Promise<void> {
    const previous = this.saved
    this.saved = { id, value: { ...value } }
    try {
      await this.transaction('readwrite', tx => {
        const meta = tx.objectStore(META)
        const revision = meta.get(REVISION)
        revision.onsuccess = () => {
          const baseline =
            previous && revision.result === previous.id
              ? previous.value
              : undefined
          const store = tx.objectStore(VALUES)
          if (!baseline) store.clear()
          Object.entries(value).forEach(([key, v]) => {
            if (!baseline || baseline[key as keyof DataModel] !== v)
              store.put(v, key)
          })
          if (baseline) {
            Object.keys(baseline).forEach(key => {
              if (!(key in value)) store.delete(key)
            })
          }
          meta.put(id, REVISION)
        }
      })
    } catch (e) {
      console.error(e)
      this.saved = undefined
      await this.transaction('readwrite', tx =>
        tx.objectStore(META).delete(REVISION),
      ).catch(console.error)
    }
  }

  async clear(): Promise<void> {
    this.saved = undefined
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
