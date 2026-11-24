import { inject, Service, signal } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import {
  arrayFromPath,
  Cursor,
  Dump,
  parseSubscriptionKey,
  pathFromArray,
} from 'patch-db-client'
import { DataModel } from './data-model'

const VALUES = 'values'
const META = 'meta'
const CURSOR = 'cursor'
const SYNCED = 'synced'
const KEYED: readonly string[] = ['packageData']
const KEYED_ROOT = Object.freeze({})
/** Data fetched from outside the server is shown only while live. */
const UNPERSISTED: readonly string[] = [
  'registryCache',
  'registryIndexes',
  'osUpdateCache',
]

type Snapshot = { id: number; roots: Record<string, Cursor> }
type Restored = { dump: Dump<DataModel>; cursors: Record<string, Cursor> }

const EMPTY_RESTORE = (): Restored => ({
  dump: { id: 0, value: {} as DataModel },
  cursors: {},
})

@Service()
export class PatchDbCache {
  private readonly win = inject(WA_WINDOW)
  private db?: Promise<IDBDatabase>
  private saved?: Snapshot & { value: DataModel }
  /** Persisted freshness requires a committed snapshot cursor. */
  readonly synced = signal<number | null>(null)

  /** Unavailable persistence yields an empty baseline for live synchronization. */
  async read(): Promise<Restored> {
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
      const { id, roots } = (cursor.result ?? {}) as Partial<Snapshot>
      if (!id || !roots) {
        this.saved = undefined
        this.synced.set(null)
        return EMPTY_RESTORE()
      }
      this.synced.set(typeof synced.result === 'number' ? synced.result : null)
      const value = fromEntries(
        keys.result.map((key, i) => [String(key), values.result[i]]),
      )
      this.saved = { id, roots, value }
      return { dump: { id, value: { ...value } }, cursors: { ...roots } }
    } catch (e) {
      console.error(e)
      this.saved = undefined
      this.synced.set(null)
      return EMPTY_RESTORE()
    }
  }

  /**
   * Stored revisions describe complete snapshots across browser tabs.
   * Every persisted root retains resumable coverage after a whole-document subscription.
   */
  async write(
    { id, value }: Dump<DataModel>,
    cursors: Record<string, Cursor>,
  ): Promise<void> {
    const root = cursors['']
    const roots = Object.fromEntries(
      [
        ...(root
          ? Object.keys(value).map(key => [pathFromArray([key]), root] as const)
          : []),
        ...Object.entries(cursors),
      ].filter(([key]) => persisted(key)),
    )
    const previous = this.saved
    this.saved = { id, roots, value: { ...value } }
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
          meta.put({ id, roots }, CURSOR)
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

function sameCursor(stored: Snapshot | undefined, saved: Snapshot): boolean {
  return (
    stored?.id === saved.id &&
    JSON.stringify(stored.roots) === JSON.stringify(saved.roots)
  )
}

/** Whether a subscription key's cursor is stored, from the root its pointer lies under. */
function persisted(key: string): boolean {
  const [root] = arrayFromPath(parseSubscriptionKey(key).pointer)
  return root !== undefined && !UNPERSISTED.includes(root)
}

function toEntries(value: DataModel): Map<string, unknown> {
  const entries = new Map<string, unknown>()
  Object.entries(value).forEach(([key, v]) => {
    if (UNPERSISTED.includes(key)) return
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
    const request = win.indexedDB.open('startos-patchdb', 3)
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
