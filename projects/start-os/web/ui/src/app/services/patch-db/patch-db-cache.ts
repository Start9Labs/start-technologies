import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { Dump } from 'patch-db-client'
import { DataModel } from './data-model'

const VALUES = 'values'
const META = 'meta'
const CURSOR = 'cursor'
const KEYED: readonly string[] = ['packageData', 'registryCache']
const KEYED_ROOT = Object.freeze({})

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
      const value = fromEntries(
        keys.result.map((key, i) => [String(key), values.result[i]]),
      )
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

function toEntries(value: DataModel): Map<string, unknown> {
  const entries = new Map<string, unknown>()
  Object.entries(value).forEach(([key, v]) => {
    const pointer = `/${encodeToken(key)}`
    if (KEYED.includes(key) && isMap(v)) {
      entries.set(pointer, KEYED_ROOT)
      Object.entries(v).forEach(([child, c]) =>
        entries.set(`${pointer}/${encodeToken(child)}`, c),
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
    const [key = '', child] = pointer.slice(1).split('/').map(decodeToken)
    if (child === undefined)
      value[key] = KEYED.includes(key) && isMap(v) ? {} : v
  })
  entries.forEach(([pointer, v]) => {
    const [key = '', child] = pointer.slice(1).split('/').map(decodeToken)
    const root = value[key]
    if (child !== undefined && isMap(root)) root[child] = v
  })
  return value as DataModel
}

function isMap(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

function encodeToken(token: string): string {
  return token.replaceAll('~', '~0').replaceAll('/', '~1')
}

function decodeToken(token: string): string {
  return token.replaceAll('~1', '/').replaceAll('~0', '~')
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
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}
