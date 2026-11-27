import { ExtendedVersion, VersionRange } from '../exver'
import * as T from '../types'

/**
 * Function signature for an uninit handler, run on every shutdown, update and uninstall.
 */
export type UninitFn = (
  effects: T.Effects,
  /**
   * @description The data version to leave the data at. Shutdown passes
   * the running version. An update, downgrade or flavor switch passes the
   * current data version, the incoming version, or the overlap of their
   * ranges, as the migration path requires, so a compatible update passes
   * the same value as a shutdown. Uninstall passes `null`, the only value
   * that identifies its exit.
   */
  target: VersionRange | ExtendedVersion | null,
) => Promise<void | null | undefined>

/** Object form of an uninit handler — implements an `uninit()` method. */
export interface UninitScript {
  uninit(
    effects: T.Effects,
    /**
     * @description The data version to leave the data at. Shutdown passes
     * the running version. An update, downgrade or flavor switch passes the
     * current data version, the incoming version, or the overlap of their
     * ranges, as the migration path requires, so a compatible update passes
     * the same value as a shutdown. Uninstall passes `null`, the only value
     * that identifies its exit.
     */
    target: VersionRange | ExtendedVersion | null,
  ): Promise<void>
}

/** Either a {@link UninitScript} object or a {@link UninitFn} function. */
export type UninitScriptOrFn = UninitScript | UninitFn

/**
 * Composes multiple uninit handlers into a single `ExpectedExports.uninit`-compatible function.
 * Handlers are executed sequentially in the order provided.
 *
 * @param uninits - One or more uninit handlers to compose
 */
export function setupUninit(
  ...uninits: UninitScriptOrFn[]
): T.ExpectedExports.uninit {
  return async opts => {
    for (const uninit of uninits) {
      if ('uninit' in uninit) await uninit.uninit(opts.effects, opts.target)
      else await uninit(opts.effects, opts.target)
    }
  }
}

/** Normalizes a {@link UninitScriptOrFn} into a {@link UninitScript} object. */
export function setupOnUninit(onUninit: UninitScriptOrFn): UninitScript {
  return 'uninit' in onUninit
    ? onUninit
    : {
        uninit: async (effects, target) => {
          await onUninit(effects, target)
        },
      }
}
