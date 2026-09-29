import { AsyncLocalStorage } from 'node:async_hooks'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import { randomBytes } from 'node:crypto'
import * as fs from 'node:fs/promises'
import { basename, dirname, resolve } from 'node:path'

const queues = new Map<string, Promise<void>>()
const heldLocks = new AsyncLocalStorage<{ target: string; held: boolean }[]>()
const execFileAsync = promisify(execFile)

export async function filePath(path: string): Promise<string> {
  try {
    return await fs.realpath(path)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
    const stat = await fs.lstat(path).catch(error => {
      if (error.code !== 'ENOENT') throw error
      return null
    })
    if (stat?.isSymbolicLink()) {
      return filePath(resolve(dirname(path), await fs.readlink(path)))
    }
    return resolve(await filePath(dirname(resolve(path))), basename(path))
  }
}

/** Runs without inheriting the caller's file locks. */
export function outsideFileLocks<A>(operation: () => A): A {
  return heldLocks.exit(operation)
}

/**
 * Reentrant for the holder. The sibling lock survives atomic replacement; its
 * inode must never be deleted.
 */
export async function withFileLock<A>(
  path: string,
  operation: (path: string) => Promise<A>,
): Promise<A> {
  const target = await filePath(path)
  const outer = heldLocks.getStore() ?? []
  if (outer.some(lock => lock.held && lock.target === target)) {
    return operation(target)
  }
  const result = (queues.get(target) ?? Promise.resolve()).then(async () => {
    await fs.mkdir(dirname(target), { recursive: true })
    const lock = resolve(dirname(target), `.${basename(target)}.startos-lock`)
    const child = spawn(
      'flock',
      [
        '--exclusive',
        '--',
        lock,
        'sh',
        '-c',
        'printf "locked\\n"; cat >/dev/null',
      ],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    )
    let stderr = ''
    let ready = false
    child.stderr.on('data', chunk => (stderr += chunk))
    const exited = new Promise<void>((resolve, reject) => {
      child.once('error', reject)
      child.once('close', code =>
        code === 0
          ? resolve()
          : reject(new Error(`File lock failed: ${stderr}`)),
      )
    })
    try {
      await new Promise<void>((resolve, reject) => {
        child.stdout.once('data', () => {
          ready = true
          resolve()
        })
        exited.then(() => {
          if (!ready) reject(new Error('File lock exited before acquisition'))
        }, reject)
      })
      const holder = { target, held: true }
      try {
        return await heldLocks.run([...outer, holder], () => operation(target))
      } finally {
        holder.held = false
      }
    } finally {
      child.stdin.end()
      await exited
    }
  })
  const tail = result.then(
    () => {},
    () => {},
  )
  queues.set(target, tail)
  void tail.then(() => {
    if (queues.get(target) === tail) queues.delete(target)
  })
  return result
}

export async function replaceFile(path: string, data: string): Promise<void> {
  const previous = await fs.stat(path).catch(error => {
    if (error.code !== 'ENOENT') throw error
    return null
  })
  const temp = resolve(
    dirname(path),
    `.${basename(path)}.${randomBytes(12).toString('hex')}.tmp`,
  )
  let created = false
  let metadataTemp: string | null = null
  try {
    const file = await fs.open(temp, 'wx', 0o600)
    created = true
    try {
      await file.writeFile(data)
      if (!previous) {
        // Default ACLs can override the process umask.
        const metadata = `${temp}.attrs`
        const empty = await fs.open(metadata, 'wx', 0o666)
        metadataTemp = metadata
        await empty.close()
      }
      await execFileAsync('cp', [
        '--attributes-only',
        '--preserve=mode,ownership,xattr',
        '--',
        metadataTemp ?? path,
        temp,
      ])
      await file.sync()
    } finally {
      await file.close()
    }
    await fs.rename(temp, path).catch(async error => {
      // A bind-mounted target cannot be replaced.
      if (error.code !== 'EBUSY') throw error
      await fs.writeFile(path, data)
    })
    const directory = await fs.open(dirname(path), 'r')
    try {
      await directory.sync()
    } finally {
      await directory.close()
    }
  } finally {
    if (created) await fs.rm(temp, { force: true })
    if (metadataTemp) await fs.rm(metadataTemp, { force: true })
  }
}
