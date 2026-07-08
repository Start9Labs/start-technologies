import * as T from '@start9labs/start-core/types'
import { asError } from '@start9labs/start-core/util/asError'
import { logErrorOnce } from '@start9labs/start-core/util/logErrorOnce'
import { Drop } from '../util'
import { SubContainer } from '../util/SubContainer'
import { CommandController } from './CommandController'
import { DaemonCommandType } from './Daemons'
import { Oneshot } from './Oneshot'

const TIMEOUT_INCREMENT_MS = 1000
const MAX_TIMEOUT_MS = 30000

/**
 * A managed long-running process wrapper around {@link CommandController}.
 *
 * When started, the daemon automatically restarts its underlying command on
 * failure with exponential backoff (up to 30 seconds). While the daemon is
 * running, it holds a use-token (`subcontainer.hold()`) on its SubContainer
 * so the container is not destroyed out from under it. There are two ways to
 * bring it down: {@link stop} releases the hold but leaves the SubContainer
 * intact so a later {@link start} can resume in it (a restartable pause);
 * {@link term} additionally destroys the SubContainer (genuine teardown).
 *
 * @typeParam Manifest - The service manifest type
 * @typeParam C - The subcontainer type, or `null` for JS-only daemons
 */
export class Daemon<
  Manifest extends T.SDKManifest,
  C extends SubContainer<Manifest> | null = SubContainer<Manifest> | null,
> extends Drop {
  private commandController: CommandController<Manifest, C> | null = null
  protected exitedSuccess = false
  private onExitFns: ((success: boolean) => void)[] = []
  private loop: { abort: AbortController; done: Promise<void> } | null = null
  private releaseSubcontainerHold: (() => Promise<void>) | null = null
  private _detached = false
  protected constructor(
    readonly subcontainer: C,
    private startCommand: () => Promise<CommandController<Manifest, C>>,
    readonly oneshot: boolean = false,
  ) {
    super()
    // The daemon owns its subcontainer's lifecycle from here on: it tears it
    // down via term() (stop the process, release the hold, then destroy), so
    // detach it from the generic per-context cleanup net — nothing else may
    // destroy it out from under the running process.
    this.subcontainer?.detach()
  }
  /** Returns true if this daemon is a one-shot process (exits after success) */
  isOneshot(): this is Oneshot<Manifest> {
    return this.oneshot
  }
  /**
   * Factory method to create a new Daemon.
   *
   * Returns a curried function: `(effects, subcontainer, exec) => Daemon`.
   * Registers an `effects.onLeaveContext` hook that terminates the daemon
   * on parent-context teardown — suppressed once {@link detach} is called,
   * which a {@link Daemons} chain does for every daemon it builds so
   * `Daemons.term()` can drive dependency-ordered shutdown instead.
   */
  static of<Manifest extends T.SDKManifest>() {
    return <C extends SubContainer<Manifest> | null>(
      effects: T.Effects,
      subcontainer: C,
      exec: DaemonCommandType<Manifest, C>,
    ) => {
      const startCommand = () =>
        CommandController.of<Manifest, C>()(effects, subcontainer, exec)
      const res = new Daemon<Manifest, C>(subcontainer, startCommand)
      effects.onLeaveContext(() => {
        if (res._detached) return
        res.term().catch(e => logErrorOnce(asError(e)))
      })
      return res
    }
  }

  /**
   * Detach this daemon from the effects context it was created under, so a
   * context-leave (`onLeaveContext`) no longer terminates it. (The
   * subcontainer is already detached at construction — the daemon owns its
   * teardown — so this only governs the daemon's own self-term.) The owner
   * then controls teardown:
   *
   * - {@link Daemons} calls this on each daemon it builds, so the chain's
   *   `term()` drives dependency-ordered shutdown rather than each daemon
   *   self-terminating on context-leave.
   * - Called directly, it lets a daemon launched from a short-lived context
   *   (e.g. an action) outlive that context — the caller then owns the
   *   lifecycle and must call {@link term} explicitly.
   *
   * Idempotent.
   */
  detach(): void {
    this._detached = true
  }
  /**
   * Start the daemon. If it is already running, this is a no-op.
   *
   * Takes a `hold()` on the daemon's SubContainer (if any) so the container
   * is kept alive while the daemon runs. The hold is released by {@link stop}
   * (a restartable pause) or {@link term} (teardown). The daemon automatically
   * restarts on failure with increasing backoff until `stop`/`term` is called.
   */
  async start() {
    if (this.loop) {
      return
    }
    if (this.subcontainer && !this.releaseSubcontainerHold) {
      this.releaseSubcontainerHold = this.subcontainer.hold()
    }
    const abort = new AbortController()
    const done = this.runLoop(abort.signal)
    this.loop = { abort, done }
  }

  private async runLoop(signal: AbortSignal) {
    let timeoutCounter = 0
    try {
      while (!signal.aborted) {
        if (this.commandController) {
          await this.commandController.term({}).catch(logErrorOnce)
          this.commandController = null
        }
        try {
          this.commandController = await this.startCommand()
          if (signal.aborted) {
            await this.commandController.term({}).catch(logErrorOnce)
            this.commandController = null
            break
          }
          const success = await this.commandController.wait().then(
            _ => true,
            err => {
              if (!signal.aborted) logErrorOnce(err)
              return false
            },
          )
          this.commandController = null
          if (signal.aborted) break
          for (const fn of this.onExitFns) {
            try {
              fn(success)
            } catch (e) {
              console.error('EXIT handler', e)
            }
          }
          if (success && this.oneshot) {
            this.exitedSuccess = true
            break
          }
        } catch (e) {
          if (!signal.aborted) console.error(e)
        }
        if (signal.aborted) break
        await new Promise<void>(resolve => {
          const timer = setTimeout(resolve, timeoutCounter)
          signal.addEventListener(
            'abort',
            () => {
              clearTimeout(timer)
              resolve()
            },
            { once: true },
          )
        })
        timeoutCounter += TIMEOUT_INCREMENT_MS
        timeoutCounter = Math.min(MAX_TIMEOUT_MS, timeoutCounter)
      }
    } finally {
      this.loop = null
    }
  }

  /**
   * Stop the daemon's process and restart loop and release the SubContainer
   * hold taken at {@link start}, **without** destroying the SubContainer.
   * A subsequent {@link start} re-takes the hold and resumes in the same
   * container.
   *
   * This is the non-destroying counterpart to {@link term}: use it for a
   * temporary stop where the daemon is expected to start again — e.g. a
   * dependency going not-ready. Releasing the hold without requesting
   * destruction leaves the SubContainer alive (its leader keeps `proc/1`),
   * so the restart is cheaper than a destroy + recreate and — as long as
   * nothing else has requested destruction — the next `start()`'s `hold()`
   * succeeds rather than throwing `already destroyed`.
   *
   * Sends the configured signal (default SIGTERM) and waits for the process
   * to exit. Idempotent, and a no-op when the daemon is not running.
   *
   * @param termOptions Optional termination settings
   * @param termOptions.signal The signal to send (default `SIGTERM`)
   * @param termOptions.timeout Milliseconds to wait before escalating to `SIGKILL` (default = `Daemon`'s configured `sigtermTimeout`, ultimately `DEFAULT_SIGTERM_TIMEOUT`)
   */
  async stop(termOptions?: {
    signal?: NodeJS.Signals | undefined
    timeout?: number | undefined
  }) {
    this.exitedSuccess = false
    this.onExitFns = []

    if (this.loop) {
      this.loop.abort.abort()
    }

    const exiting = this.commandController?.term({ ...termOptions })
    this.commandController = null
    if (exiting) await exiting.catch(logErrorOnce)

    if (this.loop) {
      await this.loop.done
    }

    if (this.releaseSubcontainerHold) {
      const release = this.releaseSubcontainerHold
      this.releaseSubcontainerHold = null
      await release().catch(logErrorOnce)
    }
  }

  /**
   * Terminate the daemon: {@link stop} it (stop the process, abort the
   * restart loop, release the SubContainer hold taken at {@link start}) and
   * then request the SubContainer's destruction.
   *
   * `destroy()` is idempotent and defers to the last hold release, so other
   * daemons sharing the same SubContainer keep it alive until they also
   * term. Unlike {@link stop}, this is a one-way teardown — a `start()`
   * after `term()` throws `already destroyed`; use {@link stop} for a
   * restartable pause.
   *
   * Sends the configured signal (default SIGTERM) and waits for the
   * process to exit.
   *
   * @param termOptions Optional termination settings
   * @param termOptions.signal The signal to send (default `SIGTERM`)
   * @param termOptions.timeout Milliseconds to wait before escalating to `SIGKILL` (default = `Daemon`'s configured `sigtermTimeout`, ultimately `DEFAULT_SIGTERM_TIMEOUT`)
   */
  async term(termOptions?: {
    signal?: NodeJS.Signals | undefined
    timeout?: number | undefined
  }) {
    await this.stop(termOptions)

    // Request the SubContainer's destruction. Idempotent + hold-aware:
    // if another daemon still holds this SubContainer, destroy waits for
    // the last hold-release before firing `destroyFs`.
    if (this.subcontainer) {
      await this.subcontainer.destroy().catch(e => logErrorOnce(asError(e)))
    }
  }

  /**
   * Register a callback to be invoked each time the daemon's process exits.
   * @param fn - Callback receiving `true` on clean exit, `false` on error
   */
  onExit(fn: (success: boolean) => void) {
    this.onExitFns.push(fn)
  }
  onDrop(): void {
    this.term().catch(e => logErrorOnce(asError(e)))
  }
}
