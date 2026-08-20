import * as fs from 'fs/promises'
import * as T from '@start9labs/start-core/types'
import * as cp from 'child_process'
import { promisify } from 'util'
import { Buffer } from 'node:buffer'
import { once } from '@start9labs/start-core/util/once'
import { Drop } from '@start9labs/start-core/util/Drop'
import { Mounts } from '../mainFn/Mounts'

export const execFile = promisify(cp.execFile)
const False = () => false

export type ExecOptions = {
  input?: string | Buffer
  /** How long to wait before SIGKILL, in ms (default 30000). `null` waits as long as the command takes — use it whenever the runtime is set by the size of the data or the speed of a disk. */
  timeout?: number | null
  /** Aborting SIGKILLs the process */
  abort?: AbortController
}

const TIMES_TO_WAIT_FOR_PROC = 100

// Returns whether the bind target was prepared as a file (vs a directory),
// so callers can pass the matching `--file` flag. For `infer` this resolves
// against the source, so it must be the single source of truth — `bind()`
// keys `--file` off this rather than re-deciding.
async function prepBind(
  from: string | null,
  to: string,
  type: 'file' | 'directory' | 'infer',
): Promise<boolean> {
  const fromMeta = from ? await fs.stat(from).catch(_ => null) : null
  const toMeta = await fs.stat(to).catch(_ => null)

  if (type === 'file' || (type === 'infer' && from && fromMeta?.isFile())) {
    if (toMeta && toMeta.isDirectory()) await fs.rmdir(to)
    if (from && !fromMeta) {
      await fs.mkdir(from.replace(/\/[^\/]*\/?$/, ''), { recursive: true })
      await fs.writeFile(from, '')
    }
    if (!toMeta) {
      await fs.mkdir(to.replace(/\/[^\/]*\/?$/, ''), { recursive: true })
      await fs.writeFile(to, '')
    }
    return true
  } else {
    if (toMeta && toMeta.isFile() && !toMeta.size) await fs.rm(to)
    if (from && !fromMeta) await fs.mkdir(from, { recursive: true })
    if (!toMeta) await fs.mkdir(to, { recursive: true })
    return false
  }
}

async function bind(
  from: string,
  to: string,
  type: 'file' | 'directory' | 'infer',
  idmap: IdMap[],
) {
  const isFile = await prepBind(from, to, type)

  // Inside the LXC subcontainer (which is itself idmapped), util-linux's
  // `mount --bind -oX-mount.idmap=...` can't reliably set up a second,
  // nested idmap. start-container's `bind-mount` subcommand performs the
  // bind via direct syscalls (open_tree + mount_setattr + move_mount) so
  // the SDK's idmap field on volume/asset/dependency mounts works.
  const args = ['bind-mount', '--source', from, '--target', to, '--recursive']
  if (isFile) {
    args.push('--file')
  }
  for (const i of idmap) {
    args.push('--idmap', `${i.fromId}:${i.toId}:${i.range}`)
  }
  await execFile('start-container', args)
}

/**
 * Isolated container environment for running service processes.
 *
 * Two concrete shapes implement this interface:
 *
 * - {@link SubContainerEager} — created and materialized immediately by
 *   {@link SubContainer.eager}. The `rootfs` / `guid` / `subpath()` accessors
 *   return `string` / `T.Guid` synchronously.
 * - {@link SubContainerLazy} — created by {@link SubContainer.of} (the
 *   default). Backing filesystem is not materialized until the first method
 *   call (or explicit {@link SubContainerLazy.eager}). `rootfs` / `guid` /
 *   `subpath()` return `Promise<...>`.
 *
 * The interface widens those accessors to `T | Promise<T>` so a caller
 * holding the interface type must `await` (which yields the value for either
 * shape). Callers holding the concrete class get the narrower sync type.
 *
 * ### Lifecycle and sharing
 *
 * Multiple consumers may share a single SubContainer (typical pattern: an
 * `Oneshot` setup step and a `Daemon` running in the same container so the
 * setup's writes are visible to the daemon). Each consumer takes a use-hold
 * via {@link SubContainer.hold} and releases it when finished. The
 * SubContainer is torn down (`destroyFs`) when {@link SubContainer.destroy}
 * has been called *and* all outstanding holds have been released — whichever
 * is later. `Daemons` manages holds for the daemons it owns.
 */
export interface SubContainer<
  Manifest extends T.SDKManifest,
  Effects extends T.Effects = T.Effects,
> extends Drop {
  /** The image this subcontainer is launched from. */
  readonly imageId: keyof Manifest['images'] & T.ImageId

  /**
   * Stable, sync handle identifying this SubContainer. Generated at
   * construction by {@link SubContainer.of} / {@link SubContainer.eager}
   * and preserved across materialization, so a lazy handle and the eager
   * it materializes into compare equal. Use this for sharing comparisons
   * that must work pre-materialization.
   */
  readonly identity: symbol

  /**
   * Absolute path to the subcontainer's rootfs. Sync `string` on
   * {@link SubContainerEager}; `Promise<string>` on
   * {@link SubContainerLazy}. Awaiting the property works for either.
   */
  readonly rootfs: string | Promise<string>

  /**
   * OS-level guid of the subcontainer. Sync `T.Guid` on
   * {@link SubContainerEager}; `Promise<T.Guid>` on
   * {@link SubContainerLazy}.
   */
  readonly guid: T.Guid | Promise<T.Guid>

  /**
   * Get the absolute path to a file or directory within this subcontainer's rootfs.
   * Sync `string` on {@link SubContainerEager}; `Promise<string>` on
   * {@link SubContainerLazy} (resolves after first materialization).
   * @param path Path relative to the rootfs
   */
  subpath(path: string): string | Promise<string>

  /**
   * Apply filesystem mounts (volumes, assets, dependencies) to this subcontainer.
   * Lazy handles materialize on this call.
   * @param mounts - The Mounts configuration to apply
   * @returns This subcontainer instance for chaining
   */
  mount(mounts: Mounts<Manifest>): Promise<this>

  /**
   * Take a use-hold on this subcontainer. The returned function releases
   * the hold; the SubContainer is destroyed when {@link destroy} has been
   * called *and* the last hold is released, in either order.
   *
   * @returns A release function — call it to drop this hold
   */
  hold(): () => Promise<void>

  /**
   * Mark this subcontainer for destruction. Fires `destroyFs` immediately
   * if no holds are outstanding; otherwise the final hold-release fires
   * it. Idempotent.
   */
  destroy(): Promise<void>

  /**
   * Detach this subcontainer from the effects context that created it, so it
   * is *not* destroyed when that context leaves (`onLeaveContext`). Use when a
   * short-lived context — e.g. an action — launches work that must outlive it;
   * after detaching, the caller owns the subcontainer's lifecycle and must
   * call {@link destroy} explicitly. Idempotent; does not affect holds or a
   * pending {@link destroy}.
   */
  detach(): void

  /**
   * @description run a command inside this subcontainer
   * DOES NOT THROW ON NONZERO EXIT CODE (see execFail)
   * @param command an array representing the command and args to execute
   * @param options env, cwd, user, stdin, `timeout` (default 30 s, `null` for no timeout), and `abort`
   * @returns
   */
  exec(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{
    throw: () => { stdout: string | Buffer; stderr: string | Buffer }
    exitCode: number | null
    exitSignal: NodeJS.Signals | null
    timedOutAfter: number | null
    stdout: string | Buffer
    stderr: string | Buffer
  }>

  /**
   * @description run a command inside this subcontainer, throwing on non-zero exit status
   * @param command an array representing the command and args to execute
   * @param options env, cwd, user, stdin, `timeout` (default 30 s, `null` for no timeout), and `abort`
   * @returns
   * @throws {@link ExitError} on non-zero exit code or signal termination
   */
  execFail(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{ stdout: string | Buffer; stderr: string | Buffer }>

  /**
   * Launch a command as the init (PID 1) process of the subcontainer.
   * Replaces the current leader process.
   * @param command - The command and arguments to execute
   * @param options - Optional environment, working directory, and user overrides
   */
  launch(
    command: string[],
    options?: CommandOptions,
  ): Promise<cp.ChildProcessWithoutNullStreams>

  /**
   * Spawn a command inside the subcontainer as a non-init process.
   * @param command - The command and arguments to execute
   * @param options - Optional environment, working directory, user, and stdio overrides
   */
  spawn(
    command: string[],
    options?: CommandOptions & StdioOptions,
  ): Promise<cp.ChildProcess>

  /**
   * @description Write a file to the subcontainer's filesystem
   * @param path Path relative to the subcontainer rootfs (e.g. "/etc/config.json")
   * @param data The data to write
   * @param options Optional write options (same as node:fs/promises writeFile)
   */
  writeFile(
    path: string,
    data:
      | string
      | NodeJS.ArrayBufferView
      | Iterable<string | NodeJS.ArrayBufferView>
      | AsyncIterable<string | NodeJS.ArrayBufferView>,
    options?: Parameters<typeof fs.writeFile>[2],
  ): Promise<void>
}

/**
 * Top-level factories for creating SubContainers. Prefer
 * {@link SubContainer.of} (lazy) unless you need synchronous access to
 * `rootfs` / `guid` / `subpath()` before running any methods — for that, use
 * {@link SubContainer.eager}.
 */
export const SubContainer = {
  /**
   * Create a {@link SubContainerLazy} that materializes its backing
   * filesystem on first use. Synchronous return — no `Promise` wrapper.
   *
   * Identity is generated at this call and threaded through to the eager
   * container produced by {@link SubContainerLazy.eager}.
   *
   * This is the default factory. Use it whenever the subcontainer might
   * not actually be needed (the canonical case: `Daemons.dynamic` may diff
   * a daemon away before it ever runs) and whenever you don't need sync
   * access to `rootfs` / `guid` / `subpath()`.
   *
   * @param effects The effects context the subcontainer will run under
   * @param image Image to launch from (must be declared in the manifest); `sharedRun` rbinds the host's `/run` if true
   * @param mounts Initial mounts applied at materialization (declared so they participate in `Daemons.dynamic`'s `configHash`)
   * @param name Debug/identification name (does not affect identity)
   */
  of<Manifest extends T.SDKManifest, Effects extends T.Effects = T.Effects>(
    effects: Effects,
    image: {
      imageId: keyof Manifest['images'] & T.ImageId
      sharedRun?: boolean
    },
    mounts: Mounts<Manifest> | null,
    name: string,
  ): SubContainerLazy<Manifest, Effects> {
    return new SubContainerLazy<Manifest, Effects>(effects, image, mounts, name)
  },

  /**
   * Create a {@link SubContainerEager}, materializing the backing
   * filesystem immediately. Returns `Promise<SubContainerEager>`.
   *
   * Use when you need synchronous access to `rootfs` / `guid` /
   * `subpath()` before running any methods (e.g. mounting paths from
   * outside the container, init-time precheck), or when you want
   * `effects.subcontainer.createFs` failures to surface here instead of at
   * first method call.
   *
   * **Do not pass an eager SubContainer to `Daemons.dynamic`'s `fn`**.
   * Eager handles created inside `fn` are wasted on re-runs that diff to
   * "leave alone" — use {@link SubContainer.of} (lazy) for daemons under
   * `Daemons.dynamic`.
   *
   * @param effects The effects context the subcontainer will run under
   * @param image Image to launch from; `sharedRun` rbinds the host's `/run` if true
   * @param mounts Mounts applied during construction
   * @param name Debug/identification name (does not affect identity)
   */
  eager<Manifest extends T.SDKManifest, Effects extends T.Effects = T.Effects>(
    effects: Effects,
    image: {
      imageId: keyof Manifest['images'] & T.ImageId
      sharedRun?: boolean
    },
    mounts: Mounts<Manifest> | null,
    name: string,
  ): Promise<SubContainerEager<Manifest, Effects>> {
    return SubContainerEager._of(
      effects,
      image,
      mounts,
      name,
      Symbol('subcontainer'),
    )
  },

  /**
   * Run a function with an ephemeral, eager subcontainer. The container
   * is created before `fn` runs and destroyed in a `finally` block after.
   * Use for one-off command execution that needs a fresh filesystem.
   *
   * @param effects The effects context the subcontainer will run under
   * @param image Image to launch from; `sharedRun` rbinds the host's `/run` if true
   * @param mounts Mounts applied during construction
   * @param name Debug/identification name (does not affect identity)
   * @param fn Function to invoke with the eager subcontainer
   * @returns Whatever `fn` returns
   */
  async withTemp<
    Manifest extends T.SDKManifest,
    T_,
    Effects extends T.Effects = T.Effects,
  >(
    effects: Effects,
    image: {
      imageId: keyof Manifest['images'] & T.ImageId
      sharedRun?: boolean
    },
    mounts: Mounts<Manifest> | null,
    name: string,
    fn: (sub: SubContainerEager<Manifest, Effects>) => Promise<T_>,
  ): Promise<T_> {
    const sub = await SubContainerEager._of(
      effects,
      image,
      mounts,
      name,
      Symbol('subcontainer'),
    )
    try {
      return await fn(sub)
    } finally {
      await sub.destroy()
    }
  },
}

/**
 * Live eager subcontainers grouped by the effects context that created them.
 * Any still alive when that context leaves (e.g. a subcontainer created in
 * `main` but never explicitly destroyed) is torn down via the armed
 * `onLeaveContext` hook instead of lingering until GC runs its `Drop`
 * finalizer.
 *
 * One hook is armed per distinct effects object — on the first subcontainer
 * created under it — and each subcontainer removes itself on destroy, so
 * repeated short-lived containers (`withTemp`, per-poll health checks) don't
 * accumulate registrations. Destroying via `destroy()` keeps the hold model
 * intact: a container still held by a running daemon defers teardown until
 * the daemon's own context-leave releases it.
 */
const contextLiveSubcontainers = new WeakMap<
  T.Effects,
  Set<SubContainerEager<any, any>>
>()

function registerForContextCleanup(
  effects: T.Effects,
  sub: SubContainerEager<any, any>,
): void {
  let live = contextLiveSubcontainers.get(effects)
  if (!live) {
    live = new Set()
    contextLiveSubcontainers.set(effects, live)
    effects.onLeaveContext(() => {
      const remaining = contextLiveSubcontainers.get(effects)
      if (!remaining) return
      for (const c of [...remaining]) {
        c.destroy().catch(e => console.error(e))
      }
    })
  }
  live.add(sub)
}

function unregisterFromContextCleanup(
  effects: T.Effects,
  sub: SubContainerEager<any, any>,
): void {
  contextLiveSubcontainers.get(effects)?.delete(sub)
}

/**
 * Want to limit what we can do in a container, so we want to launch a container with a specific image and the mounts.
 *
 * Eager subcontainer: its filesystem and leader process exist from
 * construction. `rootfs` / `guid` / `subpath()` are synchronous.
 *
 * Construct via {@link SubContainer.eager} or {@link SubContainer.withTemp}.
 */
export class SubContainerEager<
  Manifest extends T.SDKManifest,
  Effects extends T.Effects = T.Effects,
>
  extends Drop
  implements SubContainer<Manifest, Effects>
{
  private destroyed = false
  private destroyPending = false
  private holdCount = 0

  private leader: cp.ChildProcess
  private leaderExited: boolean = false
  private waitProc: () => Promise<null>

  private constructor(
    readonly effects: Effects,
    readonly imageId: keyof Manifest['images'] & T.ImageId,
    readonly rootfs: string,
    readonly guid: T.Guid,
    readonly identity: symbol,
  ) {
    super()
    this.leaderExited = false
    this.leader = cp.spawn(
      'start-container',
      ['subcontainer', 'launch', rootfs],
      {
        killSignal: 'SIGKILL',
        stdio: 'inherit',
      },
    )
    this.leader.on('exit', () => {
      this.leaderExited = true
    })
    this.waitProc = once(
      () =>
        new Promise(async (resolve, reject) => {
          let count = 0
          while (
            !(await fs.stat(`${this.rootfs}/proc/1`).then(x => !!x, False))
          ) {
            if (count++ > TIMES_TO_WAIT_FOR_PROC) {
              console.debug('Failed to start subcontainer', {
                guid: this.guid,
                imageId: this.imageId,
                rootfs: this.rootfs,
              })
              return reject(
                new Error(`Failed to start subcontainer ${this.imageId}`),
              )
            }
            await wait(1)
          }
          resolve(null)
        }),
    )
  }

  /**
   * Internal factory. Accepts an explicit `identity` so a lazy handle can
   * thread its identity through to the eager it materializes into.
   * External callers use {@link SubContainer.eager} (which generates its
   * own identity) or {@link SubContainer.of} (lazy).
   */
  static async _of<Manifest extends T.SDKManifest, Effects extends T.Effects>(
    effects: Effects,
    image: {
      imageId: keyof Manifest['images'] & T.ImageId
      sharedRun?: boolean
    },
    mounts: Mounts<Manifest> | null,
    name: string,
    identity: symbol,
  ): Promise<SubContainerEager<Manifest, Effects>> {
    const { imageId, sharedRun } = image
    const [rootfs, guid] = await effects.subcontainer.createFs({
      imageId,
      name,
    })

    const res = new SubContainerEager<Manifest, Effects>(
      effects,
      imageId,
      rootfs,
      guid,
      identity,
    )
    // Arm context cleanup before mounting — if the try below throws,
    // _destroyImmediate unregisters as it tears the partial container down.
    registerForContextCleanup(effects, res)

    try {
      if (mounts) {
        await res.mount(mounts)
      }
      const shared = ['dev', 'sys']
      if (!!sharedRun) {
        shared.push('run')
      }

      await fs.mkdir(`${rootfs}/etc`, { recursive: true })
      await fs.copyFile('/etc/resolv.conf', `${rootfs}/etc/resolv.conf`)

      for (const dirPart of shared) {
        const from = `/${dirPart}`
        const to = `${rootfs}/${dirPart}`
        await fs.mkdir(from, { recursive: true })
        await fs.mkdir(to, { recursive: true })
        await execFile('mount', ['--rbind', from, to])
      }

      return res
    } catch (e) {
      await res._destroyImmediate()
      throw e
    }
  }

  /**
   * Resolve a path within this subcontainer's rootfs (synchronous on eager).
   *
   * @param path Path relative to the rootfs (e.g. `"/etc/config.json"` or `"data/file"`)
   */
  subpath(path: string): string {
    return path.startsWith('/')
      ? `${this.rootfs}${path}`
      : `${this.rootfs}/${path}`
  }

  /**
   * Apply filesystem mounts (volumes, assets, dependencies) to
   * this subcontainer.
   *
   * @param mounts The Mounts configuration to apply
   * @returns This subcontainer instance for chaining
   */
  async mount(mounts: Mounts<Manifest>): Promise<this> {
    for (let mount of mounts.build()) {
      let { options, mountpoint } = mount
      const path = mountpoint.startsWith('/')
        ? `${this.rootfs}${mountpoint}`
        : `${this.rootfs}/${mountpoint}`
      if (options.type === 'volume') {
        const subpath = options.subpath
          ? options.subpath.startsWith('/')
            ? options.subpath
            : `/${options.subpath}`
          : '/'
        const from = `/media/startos/volumes/${options.volumeId}${subpath}`

        await bind(from, path, options.filetype, options.idmap)
      } else if (options.type === 'assets') {
        const subpath = options.subpath
          ? options.subpath.startsWith('/')
            ? options.subpath
            : `/${options.subpath}`
          : '/'
        const from = `/media/startos/assets/${subpath}`

        await bind(from, path, options.filetype, options.idmap)
      } else if (options.type === 'pointer') {
        await prepBind(null, path, 'directory')
        // The host-side mount effect applies the SDK idmap in startd, which
        // holds CAP_SYS_ADMIN over the source superblock's userns — the idmap
        // can't be set from inside the LXC against a host-mounted filesystem.
        await this.effects.mount({
          location: path,
          target: options,
        })
      } else {
        throw new Error(`unknown type ${(options as any).type}`)
      }
    }
    return this
  }

  /**
   * Take a use-hold on this subcontainer. The container is destroyed
   * when {@link destroy} has been called *and* the last hold is released,
   * in either order.
   *
   * @returns A release function — call it to drop this hold
   * @throws If this subcontainer has already been destroyed
   */
  hold(): () => Promise<void> {
    if (this.destroyed) {
      throw new Error(
        `cannot hold subcontainer ${String(this.identity.description)}: already destroyed`,
      )
    }
    this.holdCount++
    let released = false
    return async () => {
      if (released) return
      released = true
      this.holdCount--
      if (this.holdCount === 0 && this.destroyPending) {
        await this._destroyImmediate()
      }
    }
  }

  /**
   * Mark this subcontainer for destruction. Fires `destroyFs` immediately
   * if no holds are outstanding; otherwise the final hold-release fires
   * it. Idempotent.
   */
  async destroy(): Promise<void> {
    this.destroyPending = true
    if (this.holdCount === 0) await this._destroyImmediate()
  }

  /**
   * Detach from the creating context's `onLeaveContext` cleanup — drops this
   * subcontainer from its context's live set so a context-leave no longer
   * destroys it. The caller becomes responsible for calling {@link destroy}.
   * Idempotent.
   */
  detach(): void {
    unregisterFromContextCleanup(this.effects, this)
  }

  private async _destroyImmediate(): Promise<void> {
    if (this.destroyed) return
    this.destroyed = true
    unregisterFromContextCleanup(this.effects, this)
    const guid = this.guid
    await this.killLeader()
    await this.effects.subcontainer.destroyFs({ guid })
  }

  private async killLeader(): Promise<null> {
    if (this.leaderExited) return null
    return new Promise<null>((resolve, reject) => {
      try {
        let timeout = setTimeout(() => this.leader.kill('SIGKILL'), 30000)
        this.leader.on('exit', () => {
          clearTimeout(timeout)
          resolve(null)
        })
        if (!this.leader.kill('SIGTERM')) {
          reject(new Error('kill(2) failed'))
        }
      } catch (e) {
        reject(e)
      }
    })
  }

  onDrop(): void {
    if (!this.destroyed && this.holdCount === 0) {
      console.log(`Cleaning up dangling subcontainer ${this.guid}`)
      this._destroyImmediate().catch(e => console.error(e))
    }
  }

  /**
   * Run a command inside this subcontainer.
   * Does NOT throw on non-zero exit (see {@link execFail}).
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, user, cwd, stdin input, `timeout` (default 30 s, `null` for no timeout), and `abort`
   */
  async exec(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{
    throw: () => { stdout: string | Buffer; stderr: string | Buffer }
    exitCode: number | null
    exitSignal: NodeJS.Signals | null
    timedOutAfter: number | null
    stdout: string | Buffer
    stderr: string | Buffer
  }> {
    await this.waitProc()
    const imageMeta: T.ImageMetadata = await fs
      .readFile(`/media/startos/images/${this.imageId}.json`, {
        encoding: 'utf8',
      })
      .catch(() => '{}')
      .then(JSON.parse)
    let extra: string[] = []
    // what's left of `spawnOptions` is forwarded to cp.spawn; copied so the
    // deletes below can't strip the caller's object for its next call
    const { timeout = 30000, abort, ...spawnOptions } = options ?? {}
    let user = imageMeta.user || 'root'
    if (spawnOptions.user) {
      user = spawnOptions.user
      delete spawnOptions.user
    }
    let workdir = imageMeta.workdir || '/'
    if (spawnOptions.cwd) {
      workdir = spawnOptions.cwd
      delete spawnOptions.cwd
    }
    if (spawnOptions.env) {
      for (let [k, v] of Object.entries(spawnOptions.env)) {
        extra.push(`--env=${k}=${v}`)
      }
    }
    const child = cp.spawn(
      'start-container',
      [
        'subcontainer',
        'exec',
        `--env-file=/media/startos/images/${this.imageId}.env`,
        `--user=${user}`,
        `--workdir=${workdir}`,
        ...extra,
        this.rootfs,
        ...command,
      ],
      spawnOptions,
    )
    abort?.signal.addEventListener('abort', () => child.kill('SIGKILL'))
    if (spawnOptions.input) {
      await new Promise<null>((resolve, reject) => {
        try {
          child.stdin.on('error', e => reject(e))
          child.stdin.write(spawnOptions.input, e => {
            if (e) {
              reject(e)
            } else {
              resolve(null)
            }
          })
        } catch (e) {
          reject(e)
        }
      })
    }
    // Close stdin whether or not anything was written to it. No stdio override
    // is passed to cp.spawn, so the child always gets a pipe; leaving it open
    // makes any command that reads stdin to EOF block until `timeout` kills it,
    // and the caller sees only a SIGKILL that points nowhere near stdin. That
    // covers `input: ''` as much as no input at all — both are "this command
    // gets nothing", not "this command gets a stdin that never ends".
    await new Promise<null>((resolve, reject) => {
      try {
        if (!child.stdin) return resolve(null)
        child.stdin.end(() => resolve(null))
      } catch (e) {
        reject(e)
      }
    })
    const stdout = { data: '' as string }
    const stderr = { data: '' as string }
    const appendData =
      (appendTo: { data: string }) => (chunk: string | Buffer | any) => {
        if (typeof chunk === 'string' || chunk instanceof Buffer) {
          appendTo.data += chunk.toString()
        } else {
          console.error('received unexpected chunk', chunk)
        }
      }
    return new Promise((resolve, reject) => {
      child.on('error', reject)
      let killTimeout: NodeJS.Timeout | undefined
      let timedOut = false
      if (timeout !== null && child.pid) {
        killTimeout = setTimeout(() => {
          timedOut = true
          child.kill('SIGKILL')
        }, timeout)
      }
      child.stdout.on('data', appendData(stdout))
      child.stderr.on('data', appendData(stderr))
      child.on('exit', (code, signal) => {
        clearTimeout(killTimeout)
        const result = {
          exitCode: code,
          exitSignal: signal,
          // the deadline can elapse after something else already killed it
          timedOutAfter: timedOut && signal === 'SIGKILL' ? timeout : null,
          stdout: stdout.data,
          stderr: stderr.data,
        }
        resolve({
          throw: () =>
            !code && !signal
              ? { stdout: stdout.data, stderr: stderr.data }
              : (() => {
                  throw new ExitError(command[0], result)
                })(),
          ...result,
        })
      })
    })
  }

  /**
   * Run a command inside this subcontainer, throwing on non-zero exit.
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, user, cwd, stdin input, `timeout` (default 30 s, `null` for no timeout), and `abort`
   * @throws {@link ExitError} on non-zero exit code or signal termination
   */
  async execFail(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{ stdout: string | Buffer; stderr: string | Buffer }> {
    return this.exec(command, options).then(res => res.throw())
  }

  /**
   * Launch a command as the init (PID 1) process of the subcontainer.
   * Replaces the current leader process.
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, working directory, and user overrides
   */
  async launch(
    command: string[],
    options?: CommandOptions,
  ): Promise<cp.ChildProcessWithoutNullStreams> {
    await this.waitProc()
    const imageMeta: T.ImageMetadata = await fs
      .readFile(`/media/startos/images/${this.imageId}.json`, {
        encoding: 'utf8',
      })
      .catch(() => '{}')
      .then(JSON.parse)
    let extra: string[] = []
    let user = imageMeta.user || 'root'
    if (options?.user) {
      user = options.user
      delete options.user
    }
    let workdir = imageMeta.workdir || '/'
    if (options?.cwd) {
      workdir = options.cwd
      delete options.cwd
    }
    if (options?.env) {
      for (let [k, v] of Object.entries(options.env).filter(
        ([_, v]) => v != undefined,
      )) {
        extra.push(`--env=${k}=${v}`)
      }
    }
    await this.killLeader()
    this.leaderExited = false
    this.leader = cp.spawn(
      'start-container',
      [
        'subcontainer',
        'launch',
        `--env-file=/media/startos/images/${this.imageId}.env`,
        `--user=${user}`,
        `--workdir=${workdir}`,
        ...extra,
        this.rootfs,
        ...command,
      ],
      { ...options, stdio: 'inherit' },
    )
    this.leader.on('exit', () => {
      this.leaderExited = true
    })
    return this.leader as cp.ChildProcessWithoutNullStreams
  }

  /**
   * Spawn a command inside the subcontainer as a non-init process.
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, working directory, user, and stdio overrides
   */
  async spawn(
    command: string[],
    options: CommandOptions & StdioOptions = { stdio: 'inherit' },
  ): Promise<cp.ChildProcess> {
    await this.waitProc()
    const imageMeta: T.ImageMetadata = await fs
      .readFile(`/media/startos/images/${this.imageId}.json`, {
        encoding: 'utf8',
      })
      .catch(() => '{}')
      .then(JSON.parse)
    let extra: string[] = []
    let user = imageMeta.user || 'root'
    if (options?.user) {
      user = options.user
      delete options.user
    }
    let workdir = imageMeta.workdir || '/'
    if (options.cwd) {
      workdir = options.cwd
      delete options.cwd
    }
    if (options?.env) {
      for (let [k, v] of Object.entries(options.env).filter(
        ([_, v]) => v != undefined,
      )) {
        extra.push(`--env=${k}=${v}`)
      }
    }
    return cp.spawn(
      'start-container',
      [
        'subcontainer',
        'exec',
        `--env-file=/media/startos/images/${this.imageId}.env`,
        `--user=${user}`,
        `--workdir=${workdir}`,
        ...extra,
        this.rootfs,
        ...command,
      ],
      options,
    )
  }

  /**
   * Write a file to the subcontainer's filesystem.
   *
   * @param path Path relative to the subcontainer rootfs (e.g. `"/etc/config.json"`)
   * @param data The data to write
   * @param options Optional write options (same as `node:fs/promises` `writeFile`)
   */
  async writeFile(
    path: string,
    data:
      | string
      | NodeJS.ArrayBufferView
      | Iterable<string | NodeJS.ArrayBufferView>
      | AsyncIterable<string | NodeJS.ArrayBufferView>,
    options?: Parameters<typeof fs.writeFile>[2],
  ): Promise<void> {
    const fullPath = this.subpath(path)
    const dir = fullPath.replace(/\/[^/]*\/?$/, '')
    await fs.mkdir(dir, { recursive: true })
    return fs.writeFile(fullPath, data, options)
  }
}

/**
 * Lazy subcontainer: a handle whose backing filesystem is materialized on
 * first use (or explicit {@link SubContainerLazy.eager}). The narrow types
 * for `rootfs` / `guid` / `subpath()` are `Promise<...>`.
 *
 * Construct via {@link SubContainer.of}. Identity is generated at
 * construction and preserved across materialization.
 *
 * Sharing across consumers: pass the same `SubContainerLazy` to multiple
 * `addDaemon` calls. The first consumer to call a method (or `.eager()`)
 * triggers materialization; all consumers end up holding the same
 * underlying {@link SubContainerEager}.
 */
export class SubContainerLazy<
  Manifest extends T.SDKManifest,
  Effects extends T.Effects = T.Effects,
>
  extends Drop
  implements SubContainer<Manifest, Effects>
{
  readonly identity: symbol = Symbol('subcontainer')
  readonly imageId: keyof Manifest['images'] & T.ImageId
  readonly sharedRun: boolean
  readonly name: string
  /** The mounts declared at construction. Read by `Daemons.dynamic`'s configHash. */
  readonly mounts: Mounts<Manifest> | null

  private materialized: Promise<SubContainerEager<Manifest, Effects>> | null =
    null
  private destroyPending = false
  private detachPending = false

  constructor(
    readonly effects: Effects,
    image: {
      imageId: keyof Manifest['images'] & T.ImageId
      sharedRun?: boolean
    },
    mounts: Mounts<Manifest> | null,
    name: string,
  ) {
    super()
    this.imageId = image.imageId
    this.sharedRun = image.sharedRun ?? false
    this.mounts = mounts
    this.name = name
  }

  /**
   * Materialize the underlying eager subcontainer (idempotent) and return
   * it. Subsequent calls return the same instance. Useful when you need
   * the synchronous `SubContainerEager` interface (e.g. sync `rootfs` /
   * `subpath()`) — for ordinary command execution, just call `.exec()` /
   * `.writeFile()` directly and the lazy handle materializes internally.
   */
  eager(): Promise<SubContainerEager<Manifest, Effects>> {
    return (this.materialized ??= SubContainerEager._of<Manifest, Effects>(
      this.effects,
      { imageId: this.imageId, sharedRun: this.sharedRun },
      this.mounts,
      this.name,
      this.identity,
    ).then(async eager => {
      if (this.destroyPending) await eager.destroy()
      else if (this.detachPending) eager.detach()
      return eager
    }))
  }

  /** Absolute path to the materialized subcontainer's rootfs. Triggers materialization on first access. */
  get rootfs(): Promise<string> {
    return this.eager().then(e => e.rootfs)
  }

  /** OS-level guid. Triggers materialization on first access. */
  get guid(): Promise<T.Guid> {
    return this.eager().then(e => e.guid)
  }

  /**
   * Resolve a path within the subcontainer's rootfs. Triggers
   * materialization on first call.
   *
   * @param path Path relative to the rootfs
   */
  async subpath(path: string): Promise<string> {
    return (await this.eager()).subpath(path)
  }

  /**
   * Apply filesystem mounts to this subcontainer (materializes on first
   * call). Mounts beyond the ones declared at construction are not part
   * of `Daemons.dynamic`'s `configHash`.
   *
   * @param mounts The Mounts configuration to apply
   * @returns This lazy handle, for chaining
   */
  async mount(mounts: Mounts<Manifest>): Promise<this> {
    await (await this.eager()).mount(mounts)
    return this
  }

  /**
   * Take a use-hold on this subcontainer. Materializes the underlying
   * eager subcontainer asynchronously and places the hold on it.
   *
   * The returned release function is safe to call before materialization
   * completes — it cancels the pending hold so a never-materialized lazy
   * (the canonical "left alone" case in `Daemons.dynamic`) stays cheap.
   *
   * @returns A release function — call it to drop this hold
   */
  hold(): () => Promise<void> {
    // Acquire the hold on the eager once materialized. Until then we record
    // a "pending" intent so destroy() honors any outstanding holds even on
    // a never-materialized handle.
    let released = false
    let underlyingRelease: (() => Promise<void>) | null = null
    const eagerPromise = this.eager()
    const acquired = eagerPromise
      .then(eager => {
        if (released) return
        underlyingRelease = eager.hold()
      })
      .catch(e => {
        released = true
        throw e
      })
    return async () => {
      if (released) return
      released = true
      try {
        await acquired
      } catch (_) {
        // materialization failed; nothing to release
        return
      }
      if (underlyingRelease) await underlyingRelease()
    }
  }

  /**
   * Mark this subcontainer for destruction. If already materialized, the
   * underlying eager's destroy is invoked (which respects outstanding
   * holds). If never materialized, the destroy pending flag is set so
   * that any future materialization fires destroy immediately.
   */
  async destroy(): Promise<void> {
    this.destroyPending = true
    if (this.materialized) await (await this.materialized).destroy()
  }

  /**
   * Detach from the creating context's `onLeaveContext` cleanup. If already
   * materialized, detaches the underlying eager; otherwise records the intent
   * so materialization detaches immediately. Idempotent.
   */
  detach(): void {
    this.detachPending = true
    if (this.materialized) {
      this.materialized.then(e => e.detach()).catch(e => console.error(e))
    }
  }

  /**
   * Run a command inside this subcontainer (materializes on first call).
   * Does NOT throw on non-zero exit (see {@link execFail}).
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, user, cwd, stdin input, `timeout` (default 30 s, `null` for no timeout), and `abort`
   */
  async exec(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{
    throw: () => { stdout: string | Buffer; stderr: string | Buffer }
    exitCode: number | null
    exitSignal: NodeJS.Signals | null
    timedOutAfter: number | null
    stdout: string | Buffer
    stderr: string | Buffer
  }> {
    return (await this.eager()).exec(command, options)
  }

  /**
   * Run a command inside this subcontainer, throwing on non-zero exit
   * (materializes on first call).
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, user, cwd, stdin input, `timeout` (default 30 s, `null` for no timeout), and `abort`
   * @throws {@link ExitError} on non-zero exit code or signal termination
   */
  async execFail(
    command: string[],
    options?: CommandOptions & ExecOptions,
  ): Promise<{ stdout: string | Buffer; stderr: string | Buffer }> {
    return (await this.eager()).execFail(command, options)
  }

  /**
   * Launch a command as the init (PID 1) process of the subcontainer
   * (materializes on first call).
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, working directory, and user overrides
   */
  async launch(
    command: string[],
    options?: CommandOptions,
  ): Promise<cp.ChildProcessWithoutNullStreams> {
    return (await this.eager()).launch(command, options)
  }

  /**
   * Spawn a command inside the subcontainer as a non-init process
   * (materializes on first call).
   *
   * @param command Argv array representing the command and its arguments
   * @param options Optional environment, working directory, user, and stdio overrides
   */
  async spawn(
    command: string[],
    options?: CommandOptions & StdioOptions,
  ): Promise<cp.ChildProcess> {
    return (await this.eager()).spawn(command, options)
  }

  /**
   * Write a file to the subcontainer's filesystem (materializes on first call).
   *
   * @param path Path relative to the subcontainer rootfs (e.g. `"/etc/config.json"`)
   * @param data The data to write
   * @param options Optional write options (same as `node:fs/promises` `writeFile`)
   */
  async writeFile(
    path: string,
    data:
      | string
      | NodeJS.ArrayBufferView
      | Iterable<string | NodeJS.ArrayBufferView>
      | AsyncIterable<string | NodeJS.ArrayBufferView>,
    options?: Parameters<typeof fs.writeFile>[2],
  ): Promise<void> {
    return (await this.eager()).writeFile(path, data, options)
  }

  onDrop(): void {
    if (this.materialized) {
      this.materialized.then(e => e.destroy()).catch(e => console.error(e))
    }
  }
}

export type CommandOptions = {
  /** Environment variables to set for this command */
  env?: { [variable in string]?: string }
  /** the working directory to run this command in */
  cwd?: string
  /** the user to run this command as */
  user?: string
}

export type StdioOptions = {
  stdio?: cp.IOType
}

/** UID/GID mapping for mount id-remapping (see kernel idmappings docs) */
export type IdMap = { fromId: number; toId: number; range: number }

/** Union of all mount option types supported by the subcontainer runtime */
export type MountOptions =
  | MountOptionsVolume
  | MountOptionsAssets
  | MountOptionsPointer

/** Mount options for binding a service volume into a subcontainer */
export type MountOptionsVolume = {
  type: 'volume'
  volumeId: string
  subpath: string | null
  readonly: boolean
  filetype: 'file' | 'directory' | 'infer'
  idmap: IdMap[]
}

/** Mount options for binding packaged static assets into a subcontainer */
export type MountOptionsAssets = {
  type: 'assets'
  subpath: string | null
  filetype: 'file' | 'directory' | 'infer'
  idmap: { fromId: number; toId: number; range: number }[]
}

/** Mount options for binding a dependency package's volume into a subcontainer */
export type MountOptionsPointer = {
  type: 'pointer'
  packageId: string
  volumeId: string
  subpath: string | null
  readonly: boolean
  idmap: { fromId: number; toId: number; range: number }[]
}

function wait(time: number) {
  return new Promise(resolve => setTimeout(resolve, time))
}

/**
 * Error thrown when a subcontainer command exits with a non-zero code or
 * signal. Contains the full result including stdout, stderr, exit code,
 * and exit signal.
 */
export class ExitError extends Error {
  constructor(
    readonly command: string,
    readonly result: {
      exitCode: number | null
      exitSignal: T.Signals | null
      timedOutAfter: number | null
      stdout: string | Buffer
      stderr: string | Buffer
    },
  ) {
    let message: string
    if (result.exitCode) {
      message = `${command} failed with exit code ${result.exitCode}: ${result.stderr}`
    } else if (result.timedOutAfter !== null) {
      message = `${command} timed out after ${result.timedOutAfter}ms and was killed with ${result.exitSignal}: ${result.stderr}`
    } else if (result.exitSignal) {
      message = `${command} terminated with signal ${result.exitSignal}: ${result.stderr}`
    } else {
      message = `${command} succeeded: ${result.stdout}`
    }
    super(message)
  }
}
