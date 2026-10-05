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
    signal: AbortSignal
  }[]
>()
const execFileAsync = promisify(execFile)
export const UPDATE_TIMEOUT_MS = 5000
export const LOCK_TIMEOUT_MS = 10000

export async function waitForFileOperation<A>(
  pending: Promise<A>,
  signal: AbortSignal,
): Promise<A> {
  signal.throwIfAborted()
  return new Promise<A>((resolve, reject) => {
    const abort = () => reject(signal.reason)
    signal.addEventListener('abort', abort, { once: true })
    pending.then(resolve, reject).finally(() => {
      signal.removeEventListener('abort', abort)
    })
  })
}

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
  operation: (path: string, signal: AbortSignal) => Promise<A>,
): Promise<A> {
  const target = await filePath(path)
  const outer = heldLocks.getStore() ?? []
  for (const lock of outer) lock.signal.throwIfAborted()
  const signal = AbortSignal.any(outer.map(lock => lock.signal))
  if (outer.some(lock => lock.held && lock.target === target)) {
    return operation(target, signal)
  }
  const result = (queues.get(target) ?? Promise.resolve()).then(async () => {
    signal.throwIfAborted()
    const holder = { target, held: true, flocked: false, signal }
    try {
      return await heldLocks.run([...outer, holder], () =>
        operation(target, signal),
      )
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
  return waitForFileOperation(result, signal)
}

/** Aborts the callback, and all file access it starts, after `UPDATE_TIMEOUT_MS`. */
export async function withUpdateDeadline<A>(
  target: string,
  signal: AbortSignal,
  callback: () => A | Promise<A>,
): Promise<A> {
  const controller = new AbortController()
  const timer = setTimeout(
    () =>
      controller.abort(
        new Error(
          `File update timed out after ${UPDATE_TIMEOUT_MS}ms: ${target}`,
        ),
      ),
    UPDATE_TIMEOUT_MS,
  )
  const deadline = AbortSignal.any([signal, controller.signal])
  const frame = { target, held: false, flocked: false, signal: deadline }
  try {
    return await waitForFileOperation(
      heldLocks.run([...(heldLocks.getStore() ?? []), frame], async () =>
        callback(),
      ),
      deadline,
    )
  } finally {
    clearTimeout(timer)
  }
}

/** The supplied temp must become the missing target. */
export async function withFileLock<A>(
  path: string,
  operation: (
    path: string,
    temp: string | undefined,
    signal: AbortSignal,
  ) => Promise<A>,
): Promise<A> {
  return withFileQueue(path, async (target, signal) => {
    const holder = heldLocks
      .getStore()!
      .find(lock => lock.held && lock.target === target)!
    if (holder.flocked) {
      throw new Error(`Cannot perform a nested mutation on ${target}`)
    }
    return flock(target, signal, async temp => {
      holder.flocked = true
      try {
        return await operation(target, temp, signal)
      } finally {
        holder.flocked = false
      }
    })
  })
}

// Waiters can acquire an inode unlinked by the previous writer.
const flockScript = `
umask 077
while :; do
  if [ -e "$1" ]; then
    command exec 9<"$1" || {
      [ -e "$1" ] && exit 1
      continue
    }
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
  signal: AbortSignal,
  operation: (temp: string | undefined) => Promise<A>,
) {
  await fs.mkdir(dirname(target), { recursive: true })
  const temp = resolve(dirname(target), `.${basename(target)}.tmp`)
  signal.throwIfAborted()
  const child = spawn('sh', ['-ec', flockScript, 'sh', target, temp], {
    detached: true,
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  const abort = () => {
    if (!child.pid) return
    try {
      process.kill(-child.pid, 'SIGKILL')
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error
    }
  }
  const deadline = new AbortController()
  const timer = setTimeout(
    () =>
      deadline.abort(
        new Error(`File lock timed out after ${LOCK_TIMEOUT_MS}ms: ${target}`),
      ),
    LOCK_TIMEOUT_MS,
  )
  const acquiring = AbortSignal.any([signal, deadline.signal])
  acquiring.addEventListener('abort', abort, { once: true })
  let acquired = false
  let stderr = ''
  child.stderr.on('data', chunk => (stderr += chunk))
  const exited = new Promise<void>((resolve, reject) => {
    child.once('error', reject)
    child.once('close', code =>
      code === 0 ? resolve() : reject(new Error(`File lock failed: ${stderr}`)),
    )
  })
  try {
    const created = await waitForFileOperation(
      new Promise<boolean>((resolve, reject) => {
        let stdout = ''
        child.stdout.on('data', chunk => {
          stdout += chunk
          if (stdout.includes('\n')) resolve(stdout.trim() === 'new')
        })
        exited.then(
          () => reject(new Error('File lock exited before acquisition')),
          reject,
        )
      }),
      acquiring,
    )
    acquired = true
    clearTimeout(timer)
    acquiring.removeEventListener('abort', abort)
    signal.throwIfAborted()
    return await operation(created ? temp : undefined)
  } finally {
    clearTimeout(timer)
    acquiring.removeEventListener('abort', abort)
    child.stdin.end()
    await exited.catch(error => {
      if (acquired) throw error
    })
  }
}

const swept = new Set<string>()
const leftover = /^[0-9a-f]{24}\.tmp(?:\.attrs)?$/

/** Removes temps of writers that died holding the caller's lock. */
async function sweepTemps(
  path: string,
  lockedTemp: string | undefined,
): Promise<void> {
  if (lockedTemp) {
    await fs.rm(`${lockedTemp}.attrs`, { force: true })
    return
  }
  if (swept.has(path)) return
  const prefix = `.${basename(path)}.`
  for (const name of await fs.readdir(dirname(path))) {
    if (name.startsWith(prefix) && leftover.test(name.slice(prefix.length))) {
      await fs.rm(resolve(dirname(path), name), { force: true })
    }
  }
  swept.add(path)
}

export async function replaceFile(
  path: string,
  data: string,
  lockedTemp: string | undefined,
  signal: AbortSignal,
): Promise<void> {
  signal.throwIfAborted()
  await fs.mkdir(dirname(path), { recursive: true })
  await sweepTemps(path, lockedTemp)
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
      await execFileAsync(
        'cp',
        [
          '--attributes-only',
          '--preserve=mode,ownership,xattr',
          '--',
          metadataTemp ?? path,
          temp,
        ],
        { signal, killSignal: 'SIGKILL' },
      )
      await file.sync()
      // `cp` recreates a removed temp as an empty file.
      const [written, named] = await Promise.all([file.stat(), fs.stat(temp)])
      if (written.ino !== named.ino || written.dev !== named.dev) {
        throw new Error(`Temp file was replaced during the write: ${temp}`)
      }
    } finally {
      await file.close()
    }
    signal.throwIfAborted()
    await fs.rename(temp, path).catch(async error => {
      if (error.code !== 'EBUSY') throw error
      const target = await fs.open(path, 'w')
      try {
        await target.writeFile(data)
        await target.sync()
      } finally {
        await target.close()
      }
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
