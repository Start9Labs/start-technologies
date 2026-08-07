import { Signals } from '@start9labs/start-core/types'

import { HealthCheckResult } from '../health/checkFns'

import { Trigger } from '../trigger'
import * as T from '@start9labs/start-core/types'
import {
  SubContainer,
  SubContainerEager,
  SubContainerLazy,
} from '../util/SubContainer'
import { once } from '@start9labs/start-core/util/once'
import { logErrorOnce } from '@start9labs/start-core/util/logErrorOnce'
import { asError } from '@start9labs/start-core/util/asError'

import { promisify } from 'node:util'
import * as CP from 'node:child_process'

export { Daemon } from './Daemon'
export { CommandController } from './CommandController'
import { EXIT_SUCCESS, HealthDaemon } from './HealthDaemon'
import { Daemon } from './Daemon'
import { CommandController } from './CommandController'
import { Oneshot } from './Oneshot'

/** Promisified version of `child_process.exec` */
export const cpExec = promisify(CP.exec)
/** Promisified version of `child_process.execFile` */
export const cpExecFile = promisify(CP.execFile)

/**
 * Id of the sentinel {@link Daemons.runUntilSuccess} appends to the chain: a
 * oneshot depending on every user daemon, so it completes exactly when all of
 * them are ready. Not a component of the service — it publishes no health, and
 * the timeout message leaves it out.
 */
const RUN_UNTIL_SUCCESS_ID = '__RUN_UNTIL_SUCCESS'

/**
 * Configuration for a daemon's health-check readiness probe.
 *
 * Every daemon and standalone health check requires a `Ready` configuration
 * that tells StartOS how to determine whether the daemon is healthy.
 *
 * The `fn` is called on a recurring interval controlled by `trigger` (defaults
 * to 1 s before the first non-pending result, then 30 s). During the initial
 * `gracePeriod` window, `failure` results are softened to `starting` so the
 * UI doesn't flash red while a daemon is still booting.
 *
 * ### Health check result states
 *
 * | Result      | Meaning                                              | UI treatment     |
 * |-------------|------------------------------------------------------|------------------|
 * | `success`   | Healthy and fully operational                        | Green / ready    |
 * | `loading`   | Operational but still catching up (e.g. syncing)     | Progress / amber |
 * | `disabled`  | Intentionally inactive (excluded by configuration)   | Grey / skipped   |
 * | `starting`  | Not yet ready, still initializing                    | Spinner          |
 * | `waiting`   | Blocked on an external dependency                    | Spinner          |
 * | `failure`   | Unhealthy — something is wrong                       | Red / error      |
 */
export type Ready = {
  /**
   * Human-readable label shown in the StartOS health-check UI.
   * Set to `null` to hide this check from the UI entirely.
   */
  display: string | null
  /**
   * The function called on each polling interval to determine the daemon's health.
   *
   * Return a {@link HealthCheckResult} with a `result` field (`success`, `loading`,
   * `failure`, etc.) and an optional `message` string shown in the UI.
   *
   * The SDK ships several built-in helpers on `sdk.healthCheck`:
   * - `checkPortListening` — checks whether a TCP/UDP port is bound
   * - `checkWebUrl` — fetches a URL and succeeds on any HTTP response
   * - `runHealthScript` — runs a command in a subcontainer and succeeds on exit 0
   *
   * @example
   * ```ts
   * fn: () =>
   *   sdk.healthCheck.checkPortListening(effects, 80, {
   *     successMessage: 'Web server is ready',
   *     errorMessage: 'Web server is not listening',
   *   })
   * ```
   */
  fn: () => Promise<HealthCheckResult> | HealthCheckResult
  /**
   * Duration in milliseconds during which `failure` results are reported
   * as `starting` instead, giving the daemon time to initialize without
   * showing errors in the UI.
   *
   * @default 10_000
   */
  gracePeriod?: number
  /**
   * Controls the polling interval for this health check.
   *
   * Use one of the built-in triggers from `sdk.trigger`:
   * - `cooldownTrigger(ms)` — fixed interval between checks
   * - `statusTrigger({ success, loading, failure, ... })` — per-status
   *   polling intervals
   *
   * If omitted, uses the default trigger: 1 s before the first non-pending
   * result, then 30 s afterward.
   */
  trigger?: Trigger
}

/**
 * Options for running a daemon as a shell command inside a subcontainer.
 */
export type ExecCommandOptions = {
  /** The command and arguments to execute (e.g. `['bitcoind', '-conf=/etc/bitcoin.conf']`) */
  command: T.CommandType
  /**
   * How long (ms) to wait for the process to exit after sending SIGTERM
   * before force-killing it.
   *
   * @default 60_000
   */
  sigtermTimeout?: number
  /** Run the command as PID 1 inside the container (init process) */
  runAsInit?: boolean
  /** Environment variables to set for the process */
  env?: { [variable in string]?: string } | undefined
  /** Working directory for the process */
  cwd?: string | undefined
  /** Run the process as this user inside the container */
  user?: string | undefined
  /** Callback invoked with each chunk written to stdout */
  onStdout?: (chunk: Buffer | string | any) => void
  /** Callback invoked with each chunk written to stderr */
  onStderr?: (chunk: Buffer | string | any) => void
}

/**
 * Options for running a daemon via an async function that may optionally return
 * a command to execute in the subcontainer. The function receives an `AbortSignal`
 * for cooperative cancellation.
 */
export type ExecFnOptions<
  Manifest extends T.SDKManifest,
  C extends SubContainer<Manifest> | null,
> = {
  fn: (
    subcontainer: C,
    abort: AbortSignal,
  ) => Promise<C extends null ? null : ExecCommandOptions | null>
  // Defaults to the DEFAULT_SIGTERM_TIMEOUT = 60_000ms
  sigtermTimeout?: number
}

/**
 * The execution specification for a daemon: either an {@link ExecFnOptions} (async function)
 * or an {@link ExecCommandOptions} (shell command, only valid when a subcontainer is provided).
 */
export type DaemonCommandType<
  Manifest extends T.SDKManifest,
  C extends SubContainer<Manifest> | null,
> = ExecFnOptions<Manifest, C> | (C extends null ? never : ExecCommandOptions)

type NewDaemonParams<
  Manifest extends T.SDKManifest,
  C extends SubContainer<Manifest> | null,
> = {
  /** What to run as the daemon: either an async fn or a commandline command to run in the subcontainer */
  exec: DaemonCommandType<Manifest, C>
  /** The subcontainer in which the daemon runs */
  subcontainer: C
}

type OptionalParamSync<T> = T | (() => T | null)
type OptionalParamAsync<T> = () => Promise<T | null>
type OptionalParam<T> = OptionalParamSync<T> | OptionalParamAsync<T>

type AddDaemonParams<
  Manifest extends T.SDKManifest,
  Ids extends string,
  Id extends string,
  C extends SubContainer<Manifest> | null,
> = (NewDaemonParams<Manifest, C> | { daemon: Daemon<Manifest> }) & {
  ready: Ready
  /**
   * IDs of prior daemons/oneshots/health checks that must be ready before
   * this daemon starts. Enforces startup ordering in the daemon chain.
   */
  requires: Exclude<Ids, Id>[]
}

type AddOneshotParams<
  Manifest extends T.SDKManifest,
  Ids extends string,
  Id extends string,
  C extends SubContainer<Manifest> | null,
> = NewDaemonParams<Manifest, C> & {
  /**
   * IDs of prior daemons/oneshots/health checks that must be ready before
   * this oneshot runs.
   */
  requires: Exclude<Ids, Id>[]
}

type AddHealthCheckParams<Ids extends string, Id extends string> = {
  ready: Ready
  /**
   * IDs of prior daemons/oneshots/health checks that must be ready before
   * this health check starts polling.
   */
  requires: Exclude<Ids, Id>[]
}

type ErrorDuplicateId<Id extends string> = `The id '${Id}' is already used`

/** Recorder entry for a `Daemons` chain. Materialized into `HealthDaemon`s at build/reconcile time. */
type DaemonEntry<M extends T.SDKManifest> =
  | {
      kind: 'daemon'
      id: string
      subcontainer: SubContainer<M> | null
      exec: DaemonCommandType<M, SubContainer<M> | null>
      ready: Ready
      requires: ReadonlyArray<string>
      /** If supplied via the `{ daemon: ... }` form, pre-built daemon. */
      prebuiltDaemon: Daemon<M> | null
    }
  | {
      kind: 'oneshot'
      id: string
      subcontainer: SubContainer<M> | null
      exec: DaemonCommandType<M, SubContainer<M> | null>
      requires: ReadonlyArray<string>
    }
  | {
      kind: 'health'
      id: string
      ready: Ready
      requires: ReadonlyArray<string>
    }

export const runCommand = <Manifest extends T.SDKManifest>() =>
  CommandController.of<Manifest, SubContainer<Manifest>>()

/**
 * A class for defining and controlling the service daemons
 *
 * `Daemons` is a record-then-build container: each `.addDaemon(...)` /
 * `.addOneshot(...)` / `.addHealthCheck(...)` call appends an entry to an
 * internal list (no `HealthDaemon` or `SubContainer.createFs` work happens
 * yet). The chain is materialized when {@link build} is called (for static
 * topologies in {@link setupMain}) or progressively, per entry, by
 * {@link Daemons.dynamic} (for topologies that change at runtime).
 *
 * Backward-compatible with the prior eager builder: `Daemons.of(...).addDaemon(...).build()`
 * still produces a running chain. The only externally visible change is that
 * `HealthDaemon` instances exist after `build()` rather than after each
 * `.addDaemon()`.
 *
 * @example
 * ```ts
 * sdk.Daemons.of({ effects })
 *   .addDaemon('webui', {
 *     subcontainer: sdk.SubContainer.of(effects, { imageId: 'main' }, mounts, 'webui'),
 *     exec: { command: ['hello-world'] },
 *     ready: {
 *       display: 'Web Interface',
 *       // The function to run to determine the health status of the daemon
 *       fn: () =>
 *         checkPortListening(effects, 80, {
 *           successMessage: 'The web interface is ready',
 *           errorMessage: 'The web interface is not ready',
 *         }),
 *     },
 *     requires: [],
 *   })
 * ```
 */
export class Daemons<Manifest extends T.SDKManifest, Ids extends string>
  implements T.DaemonBuildable
{
  private builtHealthDaemons: HealthDaemon<Manifest>[] | null = null
  private termPromise: Promise<void> | null = null
  private detached = false

  private constructor(
    readonly effects: T.Effects,
    readonly ids: Ids[],
    readonly entries: ReadonlyArray<DaemonEntry<Manifest>>,
  ) {}

  /**
   * Returns an empty new Daemons class.
   *
   * Call .addDaemon() on the returned class to add a daemon. Daemons run in
   * the order they are defined; later daemons may depend on prior daemons.
   */
  static of<Manifest extends T.SDKManifest>(options: { effects: T.Effects }) {
    return new Daemons<Manifest, never>(options.effects, [], [])
  }

  /**
   * Build a reactive `main` entrypoint that reconciles the daemon set
   * returned by `fn` against the running set on every `effects.constRetry`
   * trigger.
   *
   * The supplied builder returns a {@link Daemons} (in record-mode, not yet
   * built); the SDK diffs its entries against the running daemons by
   * `(id, configHash)`. Per id: absent → present **start**, present →
   * absent **stop**, same hash **leave alone**, different hash **restart**.
   * Dependents of any restarted or stopped daemon are also restarted so
   * `requires` wiring stays consistent. Re-runs are coalesced.
   *
   * The diff key (`configHash`) covers the subcontainer descriptor
   * (`imageId`, `sharedRun`, `name`, structural `mounts.build()`), exec
   * (`command`, `env`, `cwd`, `user`, `runAsInit`, `sigtermTimeout`),
   * `requires` (sorted), and the structural parts of `ready` (`display`,
   * `gracePeriod`). Closures (`ready.fn`, `ready.trigger`) and pre-built
   * `Daemon` instances are intentionally excluded — surface a value
   * through one of the hashed fields if you want the reconciler to react
   * to it changing.
   *
   * **Use lazy SubContainers** ({@link SubContainer.of}) for daemons under
   * `Daemons.dynamic`. Eager handles created inside `fn` are wasted on
   * re-runs that diff to "leave alone"; the reconciler throws at hash time
   * if it sees one.
   *
   * Return the result from `setupMain` — it is a {@link T.DaemonBuildable},
   * exactly like a static `Daemons.of(...)` chain:
   *
   * ```typescript
   * export const main = sdk.setupMain(async ({ effects }) => {
   *   return sdk.Daemons.dynamic(effects, async ({ effects }) => {
   *     const sites = await storeJson.read((s) => s.sites).const(effects)
   *     return sdk.Daemons.of(effects).addDaemon(...)
   *   })
   * })
   * ```
   *
   * @param effects The effects context, from `setupMain`
   * @param fn Async builder invoked on startup and on every `constRetry`. Must return a `Daemons` in record-mode (not pre-`build()`-ed)
   * @returns A {@link DaemonsReconciler} — a `T.DaemonBuildable` to return from `setupMain`
   */
  static dynamic<Manifest extends T.SDKManifest>(
    effects: T.Effects,
    fn: DaemonsBuilder<Manifest>,
  ): DaemonsReconciler<Manifest> {
    return new DaemonsReconciler<Manifest>(effects, fn)
  }

  private appendEntry(entry: DaemonEntry<Manifest>): Daemons<Manifest, any> {
    return new Daemons<Manifest, any>(
      this.effects,
      [...this.ids, entry.id as any],
      [...this.entries, entry],
    )
  }

  /**
   * Register a long-running daemon process.
   *
   * The daemon starts in its subcontainer and is monitored by its `ready`
   * health check. Other daemons and health checks can depend on it via
   * `requires`. Pass a static options object, a sync/async factory that
   * returns options (or `null` to conditionally skip the daemon), or an
   * object with a pre-built `daemon` instance.
   *
   * @param id Unique string identifier for this daemon
   * @param options Daemon configuration, or a factory returning it (return `null` to skip)
   */
  addDaemon<Id extends string, C extends SubContainer<Manifest> | null>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamSync<AddDaemonParams<Manifest, Ids, Id, C>>,
  ): Daemons<Manifest, Ids | Id>
  addDaemon<Id extends string, C extends SubContainer<Manifest> | null>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamAsync<AddDaemonParams<Manifest, Ids, Id, C>>,
  ): Promise<Daemons<Manifest, Ids | Id>>
  addDaemon<Id extends string, C extends SubContainer<Manifest> | null>(
    id: Id,
    options: OptionalParam<AddDaemonParams<Manifest, Ids, Id, C>>,
  ) {
    const prev = this
    const res = (opts: AddDaemonParams<Manifest, Ids, Id, C> | null) => {
      if (!opts) return prev
      const entry: DaemonEntry<Manifest> = {
        kind: 'daemon',
        id,
        subcontainer:
          'daemon' in opts ? opts.daemon.subcontainer : opts.subcontainer,
        exec:
          'daemon' in opts
            ? (null as unknown as DaemonCommandType<
                Manifest,
                SubContainer<Manifest> | null
              >)
            : (opts.exec as DaemonCommandType<
                Manifest,
                SubContainer<Manifest> | null
              >),
        ready: opts.ready,
        requires: opts.requires as string[],
        prebuiltDaemon:
          'daemon' in opts ? (opts.daemon as Daemon<Manifest>) : null,
      }
      return prev.appendEntry(entry)
    }
    if (options instanceof Function) {
      const opts = options()
      if (opts instanceof Promise) return opts.then(res)
      return res(opts)
    }
    return res(options)
  }

  /**
   * Register a one-shot command that runs to completion before dependents
   * start. Common uses: `chown` for file ownership, database migrations,
   * config generation, wallet unlocking. The oneshot is considered "ready"
   * as soon as the command exits successfully (exit code 0).
   *
   * @param id Unique string identifier for this oneshot
   * @param options Oneshot configuration, or a factory returning it (return `null` to skip)
   */
  addOneshot<Id extends string, C extends SubContainer<Manifest> | null>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamSync<AddOneshotParams<Manifest, Ids, Id, C>>,
  ): Daemons<Manifest, Ids | Id>
  addOneshot<Id extends string, C extends SubContainer<Manifest> | null>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamAsync<AddOneshotParams<Manifest, Ids, Id, C>>,
  ): Promise<Daemons<Manifest, Ids | Id>>
  addOneshot<Id extends string, C extends SubContainer<Manifest> | null>(
    id: Id,
    options: OptionalParam<AddOneshotParams<Manifest, Ids, Id, C>>,
  ) {
    const prev = this
    const res = (opts: AddOneshotParams<Manifest, Ids, Id, C> | null) => {
      if (!opts) return prev
      const entry: DaemonEntry<Manifest> = {
        kind: 'oneshot',
        id,
        subcontainer: opts.subcontainer,
        exec: opts.exec as DaemonCommandType<
          Manifest,
          SubContainer<Manifest> | null
        >,
        requires: opts.requires as string[],
      }
      return prev.appendEntry(entry)
    }
    if (options instanceof Function) {
      const opts = options()
      if (opts instanceof Promise) return opts.then(res)
      return res(opts)
    }
    return res(options)
  }

  /**
   * Register a standalone health check with no associated process.
   *
   * Use this for ongoing conditions that don't map to a single daemon,
   * such as blockchain sync progress or network reachability. Dependent
   * services can reference standalone health check IDs in their
   * dependency config.
   *
   * @param id Unique string identifier for this health check
   * @param options Health check configuration, or a factory returning it (return `null` to skip)
   */
  addHealthCheck<Id extends string>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamSync<AddHealthCheckParams<Ids, Id>>,
  ): Daemons<Manifest, Ids | Id>
  addHealthCheck<Id extends string>(
    // prettier-ignore
    id:
      "" extends Id ? never :
      ErrorDuplicateId<Id> extends Id ? never :
      Id extends Ids ? ErrorDuplicateId<Id> :
      Id,
    options: OptionalParamAsync<AddHealthCheckParams<Ids, Id>>,
  ): Promise<Daemons<Manifest, Ids | Id>>
  addHealthCheck<Id extends string>(
    id: Id,
    options: OptionalParam<AddHealthCheckParams<Ids, Id>>,
  ) {
    const prev = this
    const res = (opts: AddHealthCheckParams<Ids, Id> | null) => {
      if (!opts) return prev
      const entry: DaemonEntry<Manifest> = {
        kind: 'health',
        id,
        ready: opts.ready,
        requires: opts.requires as string[],
      }
      return prev.appendEntry(entry)
    }
    if (options instanceof Function) {
      const opts = options()
      if (opts instanceof Promise) return opts.then(res)
      return res(opts)
    }
    return res(options)
  }

  /**
   * Build the daemon chain: walks the recorded entries top-down,
   * constructs `HealthDaemon`s with correct dependency wiring, registers
   * an `onLeaveContext` cleanup, and kicks off status updates.
   *
   * Idempotent — repeat calls return the same `Daemons` without rebuilding.
   *
   * **No-leak invariant**: an `onLeaveContext` shutdown handler is armed
   * *before* any daemon is constructed, and every partial construction is
   * tracked in `builtHealthDaemons` so {@link term} can clean it up.
   * Anything that throws during construction or `updateStatus` triggers
   * an immediate `term()` (which is also idempotent) before the failure
   * propagates.
   */
  async build(): Promise<this> {
    if (this.builtHealthDaemons) return this
    validateEntries(this.entries)
    // Arm cleanup BEFORE constructing anything. If anything throws below
    // — or context leaves while we're awaiting updateStatus — term() runs
    // over whatever made it into builtHealthDaemons.
    this.builtHealthDaemons = []
    this.effects.onLeaveContext(() => {
      if (this.detached) return
      this.term().catch(e => logErrorOnce(asError(e)))
    })
    const byId = new Map<string, HealthDaemon<Manifest>>()
    try {
      for (const entry of this.entries) {
        const deps = entry.requires
          .map(r => byId.get(r))
          .filter((d): d is HealthDaemon<Manifest> => !!d)
        let daemon: Daemon<Manifest> | null = null
        let readyArg: Ready | typeof EXIT_SUCCESS
        if (entry.kind === 'daemon') {
          daemon =
            entry.prebuiltDaemon ??
            Daemon.of<Manifest>()(this.effects, entry.subcontainer, entry.exec)
          readyArg = entry.ready
        } else if (entry.kind === 'oneshot') {
          daemon = Oneshot.of<Manifest>()(
            this.effects,
            entry.subcontainer,
            entry.exec,
          )
          readyArg = EXIT_SUCCESS
        } else {
          readyArg = entry.ready
        }
        // Detach the daemon so its own onLeaveContext doesn't self-term —
        // Daemons.term() drives dependency-ordered shutdown for the whole
        // chain. (Each daemon already detached its subcontainer at
        // construction.)
        daemon?.detach()
        const hd = new HealthDaemon<Manifest>(
          daemon,
          deps,
          entry.id,
          readyArg,
          this.effects,
        )
        // Push BEFORE awaiting anything — so a context-leave mid-build
        // sees this entry in builtHealthDaemons.
        this.builtHealthDaemons.push(hd)
        byId.set(entry.id, hd)
      }
      for (const hd of this.builtHealthDaemons) {
        await hd.updateStatus()
      }
    } catch (e) {
      // Best-effort tear-down of whatever we managed to construct before
      // re-throwing, so the failure path leaves no leaked daemons.
      await this.term().catch(te => logErrorOnce(asError(te)))
      throw e
    }
    return this
  }

  /** Health daemons after {@link build}, or empty before it runs. */
  get healthDaemons(): ReadonlyArray<HealthDaemon<Manifest>> {
    return this.builtHealthDaemons ?? []
  }

  /**
   * Start all registered daemons and wait until every one passes its
   * ready check, then tear everything down. Used for bootstrapping via a
   * temporary daemon chain — e.g. starting a service to call its API
   * (create admin users, register apps), then shutting it down before
   * the real daemon chain starts.
   *
   * @param timeout Maximum time (ms) to wait for all daemons to become ready, or `null` for no limit
   * @throws If the timeout is reached before all daemons are ready
   */
  async runUntilSuccess(timeout: number | null) {
    let resolve = (_: void) => {}
    let timer: ReturnType<typeof setTimeout> | undefined
    const res = new Promise<void>((res, rej) => {
      resolve = res
      if (timeout) {
        timer = setTimeout(
          () =>
            rej(
              new Error(
                `Timed out after ${timeout}ms waiting for ${this.describeNotReady()}`,
              ),
            ),
          timeout,
        )
      }
    })
    // Build the user's chain through the normal path so onLeaveContext +
    // partial-construction cleanup are in place.
    await this.build()
    const sentinelDaemon = Oneshot.of<Manifest>()(this.effects, null, {
      fn: async () => {
        resolve()
        return null
      },
    })
    sentinelDaemon.detach()
    const sentinelHd = new HealthDaemon<Manifest>(
      sentinelDaemon,
      [...(this.builtHealthDaemons ?? [])],
      RUN_UNTIL_SUCCESS_ID,
      EXIT_SUCCESS,
      this.effects,
    )
    // Inject the sentinel into the live HealthDaemon list so term() walks
    // it during dependency-ordered shutdown along with the user's daemons.
    this.builtHealthDaemons!.push(sentinelHd)
    try {
      await sentinelHd.updateStatus()
      await res
    } finally {
      clearTimeout(timer)
      await this.term()
    }
    return null
  }

  /**
   * Human-readable account of every daemon that has not reached ready, each
   * with its current health result and message, plus the count and last cause
   * of any abnormal exits.
   *
   * The exit count sits alongside the current health rather than replacing it
   * because the two disagree in the case that matters most: a `ready` check
   * that returns `loading` on a failed probe overwrites the crash back to
   * `loading` on its next poll, so health alone reads as "still starting" for
   * a process that is dying every time.
   *
   * {@link RUN_UNTIL_SUCCESS_ID} is excluded — it depends on every other
   * daemon, so it is never ready when anything else isn't.
   */
  private describeNotReady(): string {
    const notReady = (this.builtHealthDaemons ?? []).filter(
      d => !d.isReady && d.id !== RUN_UNTIL_SUCCESS_ID,
    )
    if (!notReady.length) return 'no daemons — every one became ready'
    return notReady
      .map(d => {
        const detail: string[] = [d.health.result]
        if (d.health.message) detail.push(d.health.message)
        if (d.daemon?.failedExits)
          detail.push(
            `${d.daemon.failedExits} failed exit(s), last: ${
              d.daemon.lastExitError?.message ?? 'cause unknown'
            }`,
          )
        return `${d.id} (${detail.join('; ')})`
      })
      .join(', ')
  }

  /**
   * Detach this `Daemons` from the effects context it was built under, so a
   * context-leave (`onLeaveContext`) no longer terminates the chain. Each
   * managed daemon already owns its subcontainer (detached from the context
   * at construction) and has its self-term suppressed by {@link build}, so
   * this only stops the chain's own `term()` from firing on context-leave —
   * and the whole chain then outlives the context that created it.
   *
   * Use when a short-lived context — e.g. an action — needs to launch
   * daemons that outlive it. After detaching, the caller owns the lifecycle:
   * nothing is torn down until {@link term} is called explicitly. Idempotent,
   * and safe to call before or after {@link build}.
   */
  detach(): void {
    this.detached = true
  }

  /**
   * Gracefully terminate all daemons in reverse dependency order and
   * destroy every unique SubContainer that was registered with this
   * `Daemons`. Idempotent.
   */
  async term() {
    if (!this.termPromise) {
      this.termPromise = this._term()
    }
    return this.termPromise
  }

  private async _term() {
    const hds = this.builtHealthDaemons
    if (hds) {
      const remaining = new Set(hds)
      while (remaining.size > 0) {
        const canShutdown = [...remaining].filter(
          hd =>
            ![...remaining].some(other =>
              other.dependencies.some(dep => dep.id === hd.id),
            ),
        )
        if (canShutdown.length === 0) {
          console.warn(
            'Dependency cycle detected, shutting down remaining daemons',
          )
          canShutdown.push(...[...remaining].reverse())
        }
        canShutdown.forEach(hd => remaining.delete(hd))
        await Promise.allSettled(
          canShutdown.map(async hd => {
            try {
              await hd.term()
            } catch (e) {
              console.error(e)
            }
          }),
        )
      }
    }
    // Destroy every unique subcontainer recorded across entries.
    // destroy() is idempotent and defers until the last hold is released.
    const subs = new Set<SubContainer<Manifest>>()
    for (const entry of this.entries) {
      if ('subcontainer' in entry && entry.subcontainer) {
        subs.add(entry.subcontainer)
      }
    }
    await Promise.allSettled(
      [...subs].map(s => s.destroy().catch(e => logErrorOnce(asError(e)))),
    )
  }
}

// ----------------------------------------------------------------------------
// Daemons.dynamic — reconciler + configHash
// ----------------------------------------------------------------------------

/** The user-supplied builder consumed by {@link Daemons.dynamic}. */
export type DaemonsBuilder<M extends T.SDKManifest> = (opts: {
  effects: T.Effects
}) => Promise<Daemons<M, any>> | Daemons<M, any>

/** Validate a recorded entry list: ids unique, requires reference earlier ids. */
function validateEntries<M extends T.SDKManifest>(
  entries: ReadonlyArray<DaemonEntry<M>>,
): void {
  const seen = new Set<string>()
  for (const entry of entries) {
    if (!entry.id) throw new Error(`Daemons: entry has empty id`)
    if (seen.has(entry.id)) {
      throw new Error(`Daemons: duplicate id '${entry.id}'`)
    }
    seen.add(entry.id)
    for (const req of entry.requires) {
      if (!seen.has(req)) {
        throw new Error(
          `Daemons: entry '${entry.id}' requires '${req}', which must be declared earlier`,
        )
      }
    }
  }
}

/**
 * Stable JSON hash over the structural args of a recorded entry.
 *
 * Two entries with the same id and the same `configHash` are considered
 * "the same daemon" by `Daemons.dynamic`'s reconciler — it leaves them
 * running across re-runs. Any difference triggers a restart.
 *
 * Hashed fields: kind, id, sorted requires, subcontainer descriptor
 * (`imageId`, `sharedRun`, `name`, `mounts.build()`), exec (`command`,
 * `env`, `cwd`, `user`, `runAsInit`, `sigtermTimeout`), and ready's
 * structural parts (`display`, `gracePeriod`).
 *
 * NOT hashed: `ready.fn`, `ready.trigger`, function-form `exec.fn`, and
 * any pre-built `daemon` instance.
 *
 * @param entry A recorded {@link Daemons} entry (`Daemons.entries[i]`)
 * @returns A canonical JSON string suitable for equality comparison
 */
export function configHash<M extends T.SDKManifest>(
  entry: DaemonEntry<M>,
): string {
  if (entry.kind === 'health') {
    return stableStringify({
      kind: 'health',
      id: entry.id,
      requires: [...entry.requires].sort(),
      display: entry.ready.display,
      gracePeriod: entry.ready.gracePeriod ?? null,
    })
  }
  const sub = entry.subcontainer
  const subHash =
    sub instanceof SubContainerLazy
      ? {
          imageId: sub.imageId,
          sharedRun: sub.sharedRun,
          name: sub.name,
          mounts: sub.mounts?.build() ?? [],
        }
      : sub === null
        ? null
        : { __unhashable: true }
  return stableStringify({
    kind: entry.kind,
    id: entry.id,
    requires: [...entry.requires].sort(),
    sub: subHash,
    exec: normalizeExec(entry.exec),
    ready:
      entry.kind === 'daemon'
        ? {
            display: entry.ready.display,
            gracePeriod: entry.ready.gracePeriod ?? null,
          }
        : null,
  })
}

function normalizeExec(exec: unknown): unknown {
  if (!exec || typeof exec !== 'object') return null
  if ('fn' in (exec as object)) return { __fn: true }
  const e = exec as ExecCommandOptions
  return {
    command: normalizeCommand(e.command),
    env: e.env ?? null,
    cwd: e.cwd ?? null,
    user: e.user ?? null,
    runAsInit: e.runAsInit ?? false,
    sigtermTimeout: e.sigtermTimeout ?? null,
  }
}

function normalizeCommand(c: T.CommandType): unknown {
  if (typeof c === 'string') return { kind: 'string', value: c }
  if (Array.isArray(c)) return { kind: 'argv', value: [...c] }
  if (T.isUseEntrypoint(c)) {
    return { kind: 'entrypoint', value: c.overridCmd ?? null }
  }
  return null
}

function stableStringify(v: unknown): string {
  return JSON.stringify(canonicalize(v))
}

function canonicalize(v: unknown): unknown {
  if (v === undefined || v === null) return null
  if (Array.isArray(v)) return v.map(canonicalize)
  if (typeof v === 'object') {
    const keys = Object.keys(v as object).sort()
    const out: Record<string, unknown> = {}
    for (const k of keys)
      out[k] = canonicalize((v as Record<string, unknown>)[k])
    return out
  }
  if (typeof v === 'function') return null
  return v
}

type RunningEntry<M extends T.SDKManifest> = {
  healthDaemon: HealthDaemon<M>
  subcontainer: SubContainer<M> | null
  configHash: string
  requires: ReadonlyArray<string>
}

/**
 * Reconciles a {@link Daemons} (in record-mode) against a running set on
 * every `effects.constRetry` trigger.
 *
 * Construction is internal — get one by calling {@link Daemons.dynamic}.
 * Implements {@link T.DaemonBuildable}: `build()` performs the first
 * reconcile and wires up the re-run trigger; `term()` shuts everything
 * down in dependent-first order.
 */
export class DaemonsReconciler<M extends T.SDKManifest>
  implements T.DaemonBuildable
{
  private running = new Map<string, RunningEntry<M>>()
  private inFlight: Promise<void> | null = null
  private pendingRerun = false
  private isTerminating = false
  private termPromise: Promise<void> | null = null
  private detached = false

  constructor(
    private readonly rootEffects: T.Effects,
    private readonly fn: DaemonsBuilder<M>,
  ) {}

  async build(): Promise<{ term(): Promise<void>; detach(): void }> {
    // Arm cleanup BEFORE the first reconcile. The user's `fn` is awaited
    // inside runReconcile; if context leaves during that await (or during
    // any partial startEntry), term() walks whatever made it into the
    // `running` map and tears it down.
    this.rootEffects.onLeaveContext(() => {
      if (this.detached) return
      this.term().catch(e => logErrorOnce(asError(e)))
    })
    await this.runReconcile()
    return { term: () => this.term(), detach: () => this.detach() }
  }

  /**
   * Detach the reconciler from `rootEffects`, so a context-leave no longer
   * terminates the daemons or destroys their containers. Each daemon is
   * already detached from the context as it starts (in `startEntry`), so this
   * only stops the reconciler's own `term()` from firing on context-leave.
   * After detaching, the caller owns the lifecycle and must call `term()`
   * explicitly. Idempotent.
   */
  detach(): void {
    this.detached = true
  }

  /** Trigger an out-of-band reconcile. Internal — exposed for testing. */
  scheduleRerun(): void {
    if (this.isTerminating) return
    if (this.inFlight) {
      this.pendingRerun = true
      return
    }
    this.runReconcile().catch(e => logErrorOnce(asError(e)))
  }

  private runReconcile(): Promise<void> {
    if (this.isTerminating) return Promise.resolve()
    // Tracking the in-flight promise (rather than a boolean) lets term()
    // await its completion before mutating `running` — eliminating the
    // race between teardown and a concurrent reconcile.
    const p = (async () => {
      try {
        const fnEffects = this.rootEffects.child(`dyn-daemons-fn`)
        fnEffects.constRetry = once(() => this.scheduleRerun())
        const recorded = await this.fn({ effects: fnEffects })
        validateEntries(recorded.entries)
        await this.reconcile(recorded.entries)
      } catch (e) {
        logErrorOnce(asError(e))
      }
    })()
    this.inFlight = p
    p.finally(() => {
      if (this.inFlight === p) this.inFlight = null
      if (this.pendingRerun && !this.isTerminating) {
        this.pendingRerun = false
        this.runReconcile().catch(e => logErrorOnce(asError(e)))
      }
    })
    return p
  }

  private async reconcile(
    entries: ReadonlyArray<DaemonEntry<M>>,
  ): Promise<void> {
    // Eager subcontainers can't be diffed across reconciles (no stable hash).
    // If we find any, the user's `fn` has already paid for `createFs` on
    // each one (eager construction is awaited inside fn) — destroy every
    // subc the user just handed us before throwing, so the failure leaves
    // no leaked subcontainers behind.
    const hasEager = entries.some(
      e =>
        'subcontainer' in e &&
        e.subcontainer &&
        e.subcontainer instanceof SubContainerEager,
    )
    if (hasEager) {
      await Promise.allSettled(
        entries
          .filter(
            (e): e is Extract<DaemonEntry<M>, { subcontainer: any }> =>
              'subcontainer' in e && !!e.subcontainer,
          )
          .map(e =>
            e.subcontainer!.destroy().catch(err => logErrorOnce(asError(err))),
          ),
      )
      const offender = entries.find(
        e =>
          'subcontainer' in e &&
          e.subcontainer &&
          e.subcontainer instanceof SubContainerEager,
      )!
      throw new Error(
        `Daemons.dynamic: entry '${offender.id}' uses an eager SubContainer; ` +
          `use sdk.SubContainer.of(...) (lazy) for diff-reuse under Daemons.dynamic.`,
      )
    }

    const desiredById = new Map<string, DaemonEntry<M>>(
      entries.map(e => [e.id, e]),
    )
    const desiredHashes = new Map<string, string>(
      entries.map(e => [e.id, configHash(e)]),
    )

    const toStop = new Set<string>()
    for (const [id, current] of this.running) {
      const desired = desiredById.get(id)
      if (!desired) toStop.add(id)
      else if (desiredHashes.get(id) !== current.configHash) toStop.add(id)
    }

    // Transitively mark dependents of stopping entries
    let changed = true
    while (changed) {
      changed = false
      for (const [id, current] of this.running) {
        if (toStop.has(id)) continue
        if (current.requires.some(r => toStop.has(r))) {
          toStop.add(id)
          changed = true
        }
      }
    }

    await this.stopEntries(toStop)

    // Start desired-but-not-running. Entries are in topo order by construction.
    for (const entry of entries) {
      if (this.running.has(entry.id)) continue
      try {
        await this.startEntry(entry, desiredHashes.get(entry.id)!)
      } catch (e) {
        logErrorOnce(asError(e))
      }
    }
  }

  private async stopEntries(ids: Set<string>): Promise<void> {
    if (ids.size === 0) return
    const order: string[] = []
    const visited = new Set<string>()
    const visit = (id: string) => {
      if (visited.has(id) || !ids.has(id)) return
      visited.add(id)
      for (const [otherId, other] of this.running) {
        if (other.requires.includes(id)) visit(otherId)
      }
      order.push(id)
    }
    for (const id of ids) visit(id)
    for (const id of order) {
      const cur = this.running.get(id)
      if (!cur) continue
      try {
        await cur.healthDaemon.term()
      } catch (e) {
        logErrorOnce(asError(e))
      }
      if (cur.subcontainer) {
        // destroy() is idempotent and defers until the last hold release
        // — safe even if other still-running daemons share this subcontainer.
        try {
          await cur.subcontainer.destroy()
        } catch (e) {
          logErrorOnce(asError(e))
        }
      }
      this.running.delete(id)
    }
  }

  private async startEntry(entry: DaemonEntry<M>, hash: string): Promise<void> {
    const id = entry.id
    let daemon: Daemon<M> | null = null
    let readyArg: Ready | typeof EXIT_SUCCESS
    let subcontainer: SubContainer<M> | null = null

    if (entry.kind === 'daemon') {
      subcontainer = entry.subcontainer
      daemon =
        entry.prebuiltDaemon ??
        Daemon.of<M>()(this.rootEffects, subcontainer, entry.exec)
      readyArg = entry.ready
    } else if (entry.kind === 'oneshot') {
      subcontainer = entry.subcontainer
      daemon = Oneshot.of<M>()(this.rootEffects, subcontainer, entry.exec)
      readyArg = EXIT_SUCCESS
    } else {
      readyArg = entry.ready
    }
    // Detach the daemon so its own onLeaveContext doesn't self-term — the
    // reconciler drives shutdown via stopEntries / term(). (Each daemon
    // detached its subcontainer at construction, so a per-reconcile
    // context-leave can't tear down a still-running "leave alone" daemon.)
    daemon?.detach()

    const dependencies = entry.requires
      .map(reqId => this.running.get(reqId)?.healthDaemon)
      .filter((d): d is HealthDaemon<M> => !!d)

    const healthDaemon = new HealthDaemon<M>(
      daemon,
      dependencies,
      id,
      readyArg,
      this.rootEffects,
    )

    this.running.set(id, {
      healthDaemon,
      subcontainer,
      configHash: hash,
      requires: entry.requires,
    })

    await healthDaemon.updateStatus()
  }

  private async term(): Promise<void> {
    if (this.termPromise) return this.termPromise
    this.isTerminating = true
    this.termPromise = (async () => {
      // Drain any in-flight reconcile before stopping. The reconcile
      // catches its own errors, so awaiting it never throws; the await
      // just synchronises so we don't race against startEntry/stopEntries
      // mutations to `running`.
      if (this.inFlight) {
        await this.inFlight.catch(() => {})
      }
      await this.stopEntries(new Set(this.running.keys()))
    })()
    return this.termPromise
  }
}
