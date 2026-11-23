import { inject, Service, signal } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { arrayFromPath, Dump, pathFromArray } from 'patch-db-client'
import { DataModel } from './data-model'

const VALUES = 'values'
const META = 'meta'
const CURSOR = 'cursor'
const SYNCED = 'synced'
const KEYED: readonly string[] = ['packageData', 'registryCache']
const KEYED_ROOT = Object.freeze({})

type Cursor = { id: number; epoch: string | null }

@Service()
export class PatchDbCache {
  private readonly win = inject(WA_WINDOW)
  private db?: Promise<IDBDatabase>
  private saved?: Cursor & { value: DataModel }
  epoch: string | null = null
  /** Persisted freshness requires a committed snapshot cursor. */
  readonly synced = signal<number | null>(null)

  /** Unavailable persistence yields an empty baseline for live synchronization. */
  async read(): Promise<Dump<DataModel>> {
    try {
      const { keys, values, cursor, synced } = await this.transaction(
        'readonly',
        tx => ({
          keys: tx.objectStore(VALUES).getAllKeys(),
          values: tx.objectStore(VALUES).getAll(),
          cursor: tx.objectStore(META).get(CURSOR),
          synced: tx.objectStore(META).get(SYNCED),
        }),
      )
      const { id, epoch } = (cursor.result ?? {}) as Partial<Cursor>
      if (!id) {
        this.saved = undefined
        this.synced.set(null)
        return { id: 0, value: {} as DataModel }
      }
      this.synced.set(typeof synced.result === 'number' ? synced.result : null)
      const value = fromEntries(
        keys.result.map((key, i) => [String(key), values.result[i]]),
      )
      this.epoch = epoch ?? null
      this.saved = { id, epoch: this.epoch, value }
      return { id, value: { ...value } }
    } catch (e) {
      console.error(e)
      this.saved = undefined
      this.synced.set(null)
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
            previous && sameCursor(stored.result, previous)
              ? previous.value
              : undefined
          const store = tx.objectStore(VALUES)
          const next = toEntries(value)
          const prior = baseline && toEntries(baseline)
          if (!prior) store.clear()
          next.forEach((v, key) => {
            if (!prior?.has(key) || prior.get(key) !== v) store.put(v, key)
          })
          prior?.forEach((_, key) => {
            if (!next.has(key)) store.delete(key)
          })
          meta.put({ id, epoch }, CURSOR)
        }
      })
    } catch (e) {
      console.error(e)
      this.saved = undefined
      this.synced.set(null)
      await this.transaction('readwrite', tx => {
        const meta = tx.objectStore(META)
        meta.delete(CURSOR)
        meta.delete(SYNCED)
      }).catch(console.error)
    }
  }

  async markSynced(): Promise<void> {
    const saved = this.saved
    if (!saved) return
    const time = Date.now()
    try {
      const stored = await this.transaction('readwrite', tx => {
        const meta = tx.objectStore(META)
        const cursor = meta.get(CURSOR)
        cursor.onsuccess = () => {
          if (sameCursor(cursor.result, saved)) meta.put(time, SYNCED)
        }
        return cursor
      })
      if (this.saved === saved && sameCursor(stored.result, saved))
        this.synced.set(time)
    } catch (e) {
      console.error(e)
    }
  }

  async clear(): Promise<void> {
    this.saved = undefined
    this.epoch = null
    this.synced.set(null)
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

function sameCursor(stored: Cursor | undefined, saved: Cursor): boolean {
  return stored?.id === saved.id && stored?.epoch === saved.epoch
}

function toEntries(value: DataModel): Map<string, unknown> {
  const entries = new Map<string, unknown>()
  Object.entries(value).forEach(([key, v]) => {
    const pointer = pathFromArray([key])
    if (KEYED.includes(key) && isMap(v)) {
      entries.set(pointer, KEYED_ROOT)
      Object.entries(v).forEach(([child, c]) =>
        entries.set(pathFromArray([key, child]), c),
      )
    } else {
      entries.set(pointer, v)
    }
  })
  return entries
}

function fromEntries(entries: [string, unknown][]): DataModel {
  const value: Record<string, unknown> = {}
  entries.forEach(([pointer, v]) => {
    const [key = '', child] = arrayFromPath(pointer)
    if (child === undefined)
      value[key] = KEYED.includes(key) && isMap(v) ? {} : v
  })
  entries.forEach(([pointer, v]) => {
    const [key = '', child] = arrayFromPath(pointer)
    const root = value[key]
    if (child !== undefined && isMap(root)) root[child] = v
  })
  return value as DataModel
}

function isMap(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

function open(win: Window): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = win.indexedDB.open('startos-patchdb', 2)
    request.onupgradeneeded = ({ oldVersion }) => {
      const db = request.result
      if (oldVersion) {
        db.deleteObjectStore(VALUES)
        db.deleteObjectStore(META)
      }
      db.createObjectStore(VALUES)
      db.createObjectStore(META)
    }
    request.onblocked = () => {
      request.onsuccess = () => request.result.close()
      reject(new Error('PatchDB cache upgrade is blocked by another tab'))
    }
    request.onsuccess = () => {
      const db = request.result
      db.onversionchange = () => db.close()
      resolve(db)
    }
    request.onerror = () => reject(request.error)
  })
}
