import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { Dump } from 'patch-db-client'
import { DataModel } from './data-model'

const VALUES = 'values'
const META = 'meta'
const CURSOR = 'cursor'

type Cursor = { id: number; epoch: string | null }

@Service()
export class PatchDbCache {
  private readonly win = inject(WA_WINDOW)
  private db?: Promise<IDBDatabase>
  private saved?: Cursor & { value: DataModel }
  epoch: string | null = null

  async read(): Promise<Dump<DataModel>> {
    try {
      const { keys, values, cursor } = await this.transaction(
        'readonly',
        tx => ({
          keys: tx.objectStore(VALUES).getAllKeys(),
          values: tx.objectStore(VALUES).getAll(),
          cursor: tx.objectStore(META).get(CURSOR),
        }),
      )
      const { id, epoch } = (cursor.result ?? {}) as Partial<Cursor>
      if (!id) {
        this.saved = undefined
        return { id: 0, value: {} as DataModel }
      }
      const value = Object.fromEntries(
        keys.result.map((key, i) => [key, values.result[i]]),
      ) as DataModel
      this.epoch = epoch ?? null
      this.saved = { id, epoch: this.epoch, value }
      return { id, value: { ...value } }
    } catch (e) {
      console.error(e)
      this.saved = undefined
      return { id: 0, value: {} as DataModel }
    }
  }

  /** Stored revisions describe complete snapshots across browser tabs. */
  async write({ id, value }: Dump<DataModel>): Promise<void> {
    const { epoch } = this
    const previous = this.saved
    this.saved = { id, epoch, value: { ...value } }
    try {
      await this.transaction('readwrite', tx => {
        const meta = tx.objectStore(META)
        const stored = meta.get(CURSOR)
        stored.onsuccess = () => {
          const baseline =
            previous &&
            stored.result?.id === previous.id &&
            stored.result?.epoch === previous.epoch
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
          meta.put({ id, epoch }, CURSOR)
        }
      })
    } catch (e) {
      console.error(e)
      this.saved = undefined
      await this.transaction('readwrite', tx =>
        tx.objectStore(META).delete(CURSOR),
      ).catch(console.error)
    }
  }

  async clear(): Promise<void> {
    this.saved = undefined
    this.epoch = null
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
