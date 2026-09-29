import { watch, type FSWatcher, type Stats } from 'node:fs'
import * as fs from 'node:fs/promises'
import { basename, dirname } from 'node:path'
import { outsideFileLocks, withFileLock } from './fileAccess'

const mounts = new Map<string, Set<FileMount>>()

type FileMount = {
  active: boolean
  refresh: () => Promise<void>
  pending: Set<Promise<void>>
}

async function track(mount: FileMount, pending: Promise<void>): Promise<void> {
  mount.pending.add(pending)
  try {
    await pending
  } finally {
    mount.pending.delete(pending)
  }
}

export function hasFileMounts(path: string): boolean {
  return mounts.has(path)
}

/**
 * Logs failures; commands retry through `FileMounts.sync()`.
 * The caller must hold the source's `withFileLock` through refresh.
 */
export async function refreshFileMounts(path: string): Promise<void> {
  for (const mount of mounts.get(path) ?? []) {
    if (mount.active) await mount.refresh().catch(console.error)
  }
}

async function sourceStat(path: string): Promise<Stats | null> {
  return fs.stat(path).catch(error => {
    if (error.code !== 'ENOENT') throw error
    return null
  })
}

export class FileMounts {
  private readonly registrations: {
    path: string
    mount: FileMount
    watcher: FSWatcher
  }[] = []

  async add(
    source: string,
    target: string,
    rebind: () => Promise<void>,
  ): Promise<void> {
    await withFileLock(source, async path => {
      const mount: FileMount = {
        active: true,
        pending: new Set(),
        refresh: async () => {
          const pending = (async () => {
            while (mount.active) {
              const [from, to] = await Promise.all([
                sourceStat(path),
                fs.stat(target),
              ])
              if (!from || (from.dev === to.dev && from.ino === to.ino)) return
              await rebind()
            }
          })()
          await track(mount, pending)
        },
      }
      const watcher = outsideFileLocks(() =>
        watch(dirname(path), { persistent: false }, (_, name) => {
          if (
            !mount.active ||
            (name !== null && name.toString() !== basename(path))
          )
            return
          void track(
            mount,
            withFileLock(path, async () => {
              if (mount.active) await mount.refresh()
            }),
          ).catch(error => {
            if (mount.active) console.error(error)
          })
        }),
      )
      watcher.on('error', error => console.error(error))
      let set = mounts.get(path)
      if (!set) {
        set = new Set()
        mounts.set(path, set)
      }
      set.add(mount)
      this.registrations.push({ path, mount, watcher })
      await mount.refresh()
    })
  }

  async sync(): Promise<void> {
    for (const { path, mount } of this.registrations) {
      await track(
        mount,
        withFileLock(path, async () => {
          if (mount.active) await mount.refresh()
        }),
      )
    }
  }

  async close(): Promise<void> {
    const registrations = this.registrations.splice(0)
    for (const { path, mount, watcher } of registrations) {
      mount.active = false
      watcher.close()
      const set = mounts.get(path)
      set?.delete(mount)
      if (!set?.size) mounts.delete(path)
    }
    await Promise.all(
      registrations.flatMap(({ mount }) =>
        [...mount.pending].map(pending => pending.catch(() => {})),
      ),
    )
  }
}
