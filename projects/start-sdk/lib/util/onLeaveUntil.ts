import * as T from '@start9labs/start-core/types'

const leaveCallbacks = new WeakMap<T.Effects, Set<() => void>>()

/**
 * Runs the callback when the context leaves, unless released first.
 * The caller must release it on completion.
 */
export function onLeaveUntil(effects: T.Effects, fn: () => void): () => void {
  const existing = leaveCallbacks.get(effects)
  const callbacks = existing ?? new Set<() => void>()
  if (!existing) {
    leaveCallbacks.set(effects, callbacks)
    effects.onLeaveContext(() => {
      for (const callback of callbacks) callback()
      callbacks.clear()
    })
  }
  callbacks.add(fn)
  return () => {
    callbacks.delete(fn)
  }
}
