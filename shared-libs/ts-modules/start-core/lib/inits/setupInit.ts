import { VersionRange } from '../exver'
import * as T from '../types'
import { once } from '../util'
import { FullProgressTracker } from '../util/FullProgressTracker'

/**
 * The reason a service's init function is being called:
 * - `'install'` — first-time installation
 * - `'update'` — after a package update
 * - `'restore'` — after restoring from backup
 * - `null` — regular startup or reactive re-run
 */
export type InitKind = 'install' | 'update' | 'restore' | null

/**
 * Function signature for an init handler that runs during service startup.
 *
 * `progress` is this handler's own {@link FullProgressTracker}, created by the
 * harness. Add phases to it and update them — updates auto-report to the
 * install/update UI in the background, so you never touch the effect. Call
 * `progress.sync()` only to force a flush. Safe to ignore if there's nothing
 * to report.
 */
export type InitFn<Kind extends InitKind = InitKind> = (
  effects: T.Effects,
  kind: Kind,
  progress: FullProgressTracker,
) => Promise<void | null | undefined>

/** Object form of an init handler — implements an `init()` method. */
export interface InitScript<Kind extends InitKind = InitKind> {
  init(
    effects: T.Effects,
    kind: Kind,
    progress?: FullProgressTracker,
  ): Promise<void>
}

/** Either an {@link InitScript} object or an {@link InitFn} function. */
export type InitScriptOrFn<Kind extends InitKind = InitKind> =
  | InitScript<Kind>
  | InitFn<Kind>

/**
 * Composes multiple init handlers into a single `ExpectedExports.init`-compatible function.
 * Handlers are executed sequentially in the order provided.
 *
 * @param inits - One or more init handlers to compose
 */
/** Reruns only this handler when its watched values change; subsequent runs receive a null kind and detached progress. */
export async function runReactiveInit(
  effects: T.Effects,
  name: string,
  init: InitScriptOrFn,
  kind: InitKind,
  progress?: FullProgressTracker,
): Promise<void> {
  let firstRun = true
  const run = async () => {
    const runKind = firstRun ? kind : null
    const runProgress = firstRun ? progress : new FullProgressTracker()
    firstRun = false
    let complete: () => void = () => {}
    const settled = new Promise<void>(resolve => {
      complete = resolve
    })
    const child = effects.child(name)
    child.constRetry = once(() =>
      settled.then(() => run()).catch(console.error),
    )
    try {
      if ('init' in init) await init.init(child, runKind, runProgress)
      else await init(child, runKind, runProgress as FullProgressTracker)
    } finally {
      complete()
    }
  }
  await run()
}

export function setupInit(...inits: InitScriptOrFn[]): T.ExpectedExports.init {
  return async opts => {
    // One root tracker, shared across all inits — each handler adds its own
    // phases (with its own names) to it, unaware of the others. The effects
    // context is baked into the sink, and phase updates auto-sync in the
    // background; we only flush at the end.
    const tracker = new FullProgressTracker(progress =>
      opts.effects.setInitProgress({ progress }),
    )

    for (const idx in inits) {
      await runReactiveInit(
        opts.effects,
        `init_${idx}`,
        inits[idx],
        opts.kind,
        tracker,
      )
    }
    tracker.complete()
    await tracker.sync()
  }
}

/** Normalizes an {@link InitScriptOrFn} into an {@link InitScript} object. */
export function setupOnInit(onInit: InitScriptOrFn): InitScript {
  return 'init' in onInit
    ? onInit
    : {
        init: async (effects, kind, progress) => {
          await onInit(effects, kind, progress as FullProgressTracker)
        },
      }
}
