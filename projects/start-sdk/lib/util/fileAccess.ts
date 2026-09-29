import { AsyncLocalStorage } from 'node:async_hooks'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import { randomBytes } from 'node:crypto'
import * as fs from 'node:fs/promises'
import { basename, dirname, resolve } from 'node:path'

const queues = new Map<string, Promise<void>>()
const heldLocks = new AsyncLocalStorage<
  { target: string; held: boolean; flocked: boolean }[]
>()
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

/** Serializes access within this process; reentrant for the holder. */
export async function withFileQueue<A>(
  path: string,
  operation: (path: string) => Promise<A>,
): Promise<A> {
  const target = await filePath(path)
  const outer = heldLocks.getStore() ?? []
  if (outer.some(lock => lock.held && lock.target === target)) {
    return operation(target)
  }
  const result = (queues.get(target) ?? Promise.resolve()).then(async () => {
    const holder = { target, held: true, flocked: false }
    try {
      return await heldLocks.run([...outer, holder], () => operation(target))
    } finally {
      holder.held = false
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

/** Also excludes other processes through a sibling lock file removed on release. */
export async function withFileLock<A>(
  path: string,
  operation: (path: string) => Promise<A>,
): Promise<A> {
  return withFileQueue(path, async target => {
    const holder = heldLocks
      .getStore()!
      .find(lock => lock.held && lock.target === target)!
    if (holder.flocked) return operation(target)
    return flock(target, async () => {
      holder.flocked = true
      try {
        return await operation(target)
      } finally {
        holder.flocked = false
      }
    })
  })
}

// Retries until the locked inode is the one at the path; the holder unlinks
// the lock file before releasing it.
const flockScript = `
while :; do
  exec 9>>"$1"
  flock --exclusive 9
  [ /proc/$$/fd/9 -ef "$1" ] && break
  exec 9>&-
done
printf 'locked\\n'
cat >/dev/null
rm -f "$1"
`

async function flock<A>(target: string, operation: () => Promise<A>) {
  await fs.mkdir(dirname(target), { recursive: true })
  const lock = resolve(dirname(target), `.${basename(target)}.startos-lock`)
  const child = spawn('sh', ['-ec', flockScript, 'sh', lock], {
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  let stderr = ''
  let ready = false
  child.stderr.on('data', chunk => (stderr += chunk))
  const exited = new Promise<void>((resolve, reject) => {
    child.once('error', reject)
    child.once('close', code =>
      code === 0 ? resolve() : reject(new Error(`File lock failed: ${stderr}`)),
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
    return await operation()
  } finally {
    child.stdin.end()
    await exited
  }
}

export async function replaceFile(path: string, data: string): Promise<void> {
  await fs.mkdir(dirname(path), { recursive: true })
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
