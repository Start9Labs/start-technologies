import { AsyncLocalStorage } from 'node:async_hooks'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import { randomBytes } from 'node:crypto'
import * as fs from 'node:fs/promises'
import { basename, dirname, resolve } from 'node:path'

const queues = new Map<string, Promise<void>>()
const heldLocks = new AsyncLocalStorage<
  {
    target: string
    held: boolean
    flocked: boolean
    temp?: string
  }[]
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
    const holder = { target, held: true, flocked: false, temp: undefined }
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

/**
 * Also excludes other processes by locking the file itself. For a missing
 * file, the second argument is the locked temp file that must become it.
 */
export async function withFileLock<A>(
  path: string,
  operation: (path: string, temp: string | undefined) => Promise<A>,
): Promise<A> {
  return withFileQueue(path, async target => {
    const holder = heldLocks
      .getStore()!
      .find(lock => lock.held && lock.target === target)!
    if (holder.flocked) return operation(target, holder.temp)
    return flock(target, async temp => {
      holder.flocked = true
      holder.temp = temp
      try {
        return await operation(target, temp)
      } finally {
        holder.flocked = false
        holder.temp = undefined
      }
    })
  })
}

// Retries until the locked inode is the one at the target, or at the temp
// path while the target is missing. An unused temp is removed under its lock.
const flockScript = `
umask 077
while :; do
  if [ -e "$1" ]; then
    command exec 9<"$1" 2>/dev/null || continue
  else
    exec 9>>"$2"
  fi
  flock --exclusive 9
  if [ -e "$1" ]; then
    [ /proc/$$/fd/9 -ef "$1" ] && echo existing && break
  else
    [ /proc/$$/fd/9 -ef "$2" ] && echo new && break
  fi
  if [ /proc/$$/fd/9 -ef "$2" ]; then rm -f "$2"; fi
  exec 9<&-
done
cat >/dev/null
if [ /proc/$$/fd/9 -ef "$2" ]; then rm -f "$2"; fi
`

async function flock<A>(
  target: string,
  operation: (temp: string | undefined) => Promise<A>,
) {
  await fs.mkdir(dirname(target), { recursive: true })
  const temp = resolve(dirname(target), `.${basename(target)}.tmp`)
  const child = spawn('sh', ['-ec', flockScript, 'sh', target, temp], {
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  let stderr = ''
  child.stderr.on('data', chunk => (stderr += chunk))
  const exited = new Promise<void>((resolve, reject) => {
    child.once('error', reject)
    child.once('close', code =>
      code === 0 ? resolve() : reject(new Error(`File lock failed: ${stderr}`)),
    )
  })
  try {
    const created = await new Promise<boolean>((resolve, reject) => {
      let stdout = ''
      child.stdout.on('data', chunk => {
        stdout += chunk
        if (stdout.includes('\n')) resolve(stdout.trim() === 'new')
      })
      exited.then(
        () => reject(new Error('File lock exited before acquisition')),
        reject,
      )
    })
    return await operation(created ? temp : undefined)
  } finally {
    child.stdin.end()
    await exited
  }
}

/** Writes through the given temp file when one is supplied. */
export async function replaceFile(
  path: string,
  data: string,
  lockedTemp?: string,
): Promise<void> {
  await fs.mkdir(dirname(path), { recursive: true })
  const previous = await fs.stat(path).catch(error => {
    if (error.code !== 'ENOENT') throw error
    return null
  })
  const temp =
    lockedTemp ??
    resolve(
      dirname(path),
      `.${basename(path)}.${randomBytes(12).toString('hex')}.tmp`,
    )
  let created = false
  let metadataTemp: string | null = null
  try {
    const file = await fs.open(temp, lockedTemp ? 'w' : 'wx', 0o600)
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
