import * as T from '@start9labs/start-core/types'
import { Daemons } from '../mainFn/Daemons'
import { onceTrigger } from '../trigger'

type Manifest = T.SDKManifest

const settle = () => new Promise<void>(resolve => setImmediate(resolve))

const ready = (display: string | null) => ({
  display,
  gracePeriod: 0,
  trigger: onceTrigger,
  fn: () => ({ result: 'success' as const, message: null }),
})

function makeRuntime() {
  type Scope = {
    active: boolean
    leave: (() => void | null | undefined)[]
    children: Map<string, Scope>
  }
  const scope = (): Scope => ({ active: true, leave: [], children: new Map() })
  const close = (node: Scope) => {
    node.active = false
    for (const child of node.children.values()) close(child)
    for (const fn of node.leave) fn()
  }

  const root = scope()
  const events: string[] = []
  const health = new Map<string, Parameters<T.Effects['setHealth']>[0]>()
  const setHealth = jest.fn(
    async (params: Parameters<T.Effects['setHealth']>[0]) => {
      events.push(`setHealth:${params.id}:${params.result}`)
      health.set(params.id, params)
      return null
    },
  )
  const clearHealth = jest.fn(
    async (params: Parameters<T.Effects['clearHealth']>[0]) => {
      if ('only' in params) {
        events.push(`clearHealth:only:${params.only.join(',')}`)
        for (const id of params.only) health.delete(id)
      } else {
        events.push(`clearHealth:except:${params.except.join(',')}`)
        const keep = new Set(params.except)
        for (const id of [...health.keys()]) {
          if (!keep.has(id)) health.delete(id)
        }
      }
      return null
    },
  )

  const effectsFor = (node: Scope): T.Effects =>
    ({
      eventId: null,
      child: name => {
        const previous = node.children.get(name)
        if (previous) close(previous)
        const next = scope()
        node.children.set(name, next)
        return effectsFor(next)
      },
      get isInContext() {
        return node.active
      },
      onLeaveContext: fn => {
        node.leave.push(fn)
      },
      setHealth,
      clearHealth,
      getSharedVolumes: async () => [],
    }) satisfies Partial<T.Effects> as unknown as T.Effects

  return {
    effects: effectsFor(root),
    events,
    health,
    setHealth,
    clearHealth,
    close: () => close(root),
  }
}

function blockingExec(events: string[], id: string) {
  return {
    fn: (_sub: null, abort: AbortSignal) =>
      new Promise<null>(resolve => {
        events.push(`exec:start:${id}`)
        const stop = () => {
          events.push(`exec:stop:${id}`)
          resolve(null)
        }
        if (abort.aborted) return stop()
        abort.addEventListener('abort', stop, { once: true })
      }),
  }
}

function makeWorker(
  events: string[],
  workers: ReadonlyArray<string>,
  worker: string,
) {
  return workers.includes(worker)
    ? {
        subcontainer: null,
        exec: blockingExec(events, worker),
        ready: ready(worker),
        requires: [] as [],
      }
    : null
}

describe('Daemons.dynamic health clearing', () => {
  it('clears the removed daemon health entry after term while keeping remaining daemons healthy', async () => {
    const rt = makeRuntime()
    let workers: Array<'worker-a' | 'worker-b'> = ['worker-a', 'worker-b']
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects })
        .addDaemon('worker-a', () => makeWorker(rt.events, workers, 'worker-a'))
        .addDaemon('worker-b', () =>
          makeWorker(rt.events, workers, 'worker-b'),
        ),
    )

    const built = await reconciler.build()
    try {
      await settle()
      expect([...rt.health.keys()].sort()).toEqual(['worker-a', 'worker-b'])
      expect(rt.health.get('worker-a')?.result).toBe('success')
      expect(rt.health.get('worker-b')?.result).toBe('success')

      rt.events.length = 0
      workers = ['worker-a']
      reconciler.scheduleRerun()
      await settle()

      expect(rt.clearHealth).toHaveBeenCalledTimes(1)
      expect(rt.clearHealth).toHaveBeenCalledWith({ only: ['worker-b'] })
      expect(rt.events.indexOf('exec:stop:worker-b')).toBeGreaterThanOrEqual(0)
      expect(rt.events.indexOf('exec:stop:worker-b')).toBeLessThan(
        rt.events.indexOf('clearHealth:only:worker-b'),
      )
      expect(rt.events).not.toContain('exec:stop:worker-a')
      expect([...rt.health.keys()]).toEqual(['worker-a'])
      expect(rt.health.get('worker-a')?.result).toBe('success')
    } finally {
      await built.term()
      rt.close()
    }
  })

  it('clears a removed standalone health check', async () => {
    const rt = makeRuntime()
    let includeSync = true
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) => {
      const daemons = Daemons.of<Manifest>({ effects }).addHealthCheck(
        'primary',
        {
          ready: ready('Primary'),
          requires: [],
        },
      )
      return includeSync
        ? daemons.addHealthCheck('sync', {
            ready: ready('Sync'),
            requires: [],
          })
        : daemons
    })

    const built = await reconciler.build()
    try {
      await settle()
      expect([...rt.health.keys()].sort()).toEqual(['primary', 'sync'])

      includeSync = false
      reconciler.scheduleRerun()
      await settle()

      expect(rt.clearHealth).toHaveBeenCalledTimes(1)
      expect(rt.clearHealth).toHaveBeenCalledWith({ only: ['sync'] })
      expect([...rt.health.keys()]).toEqual(['primary'])
    } finally {
      await built.term()
      rt.close()
    }
  })

  it.each(['hidden', 'oneshot'] as const)(
    'clears stale health when an entry transitions from displayed to %s',
    async target => {
      const rt = makeRuntime()
      let mode: 'displayed' | 'hidden' | 'oneshot' = 'displayed'
      const reconciler = Daemons.dynamic<Manifest>(
        rt.effects,
        ({ effects }) => {
          const daemons = Daemons.of<Manifest>({ effects })
          if (mode === 'oneshot') {
            return daemons.addOneshot('task', {
              subcontainer: null,
              exec: { fn: async () => null },
              requires: [],
            })
          }
          return daemons.addDaemon('task', {
            subcontainer: null,
            exec: blockingExec(rt.events, 'task'),
            ready: ready(mode === 'displayed' ? 'Task' : null),
            requires: [],
          })
        },
      )

      const built = await reconciler.build()
      try {
        await settle()
        expect([...rt.health.keys()]).toEqual(['task'])

        mode = target
        reconciler.scheduleRerun()
        await settle()

        expect(rt.clearHealth).toHaveBeenCalledTimes(1)
        expect(rt.clearHealth).toHaveBeenCalledWith({ only: ['task'] })
        expect([...rt.health.keys()]).toEqual([])
      } finally {
        await built.term()
        rt.close()
      }
    },
  )

  it('does not call clearHealth when restarting an entry that keeps a display', async () => {
    const rt = makeRuntime()
    let revision = 1
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects }).addDaemon('worker', {
        subcontainer: null,
        exec: blockingExec(rt.events, `worker-${revision}`),
        uses: revision,
        ready: ready('Worker'),
        requires: [],
      }),
    )

    const built = await reconciler.build()
    try {
      await settle()
      expect(rt.health.get('worker')?.result).toBe('success')

      rt.setHealth.mockClear()
      revision = 2
      reconciler.scheduleRerun()
      await settle()

      expect(rt.clearHealth).not.toHaveBeenCalled()
      expect(rt.setHealth.mock.calls.map(([arg]) => arg.result)).toEqual([
        'starting',
        'success',
      ])
    } finally {
      await built.term()
      rt.close()
    }
  })

  it('does not call clearHealth when removing hidden-display or oneshot entries', async () => {
    const rt = makeRuntime()
    let includeEntries = true
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) => {
      const daemons = Daemons.of<Manifest>({ effects })
      if (!includeEntries) return daemons
      return daemons
        .addOneshot('setup', {
          subcontainer: null,
          exec: { fn: async () => null },
          requires: [],
        })
        .addDaemon('hidden', {
          subcontainer: null,
          exec: blockingExec(rt.events, 'hidden'),
          ready: ready(null),
          requires: [],
        })
    })

    const built = await reconciler.build()
    try {
      await settle()
      includeEntries = false
      reconciler.scheduleRerun()
      await settle()

      expect(rt.clearHealth).not.toHaveBeenCalled()
    } finally {
      await built.term()
      rt.close()
    }
  })

  it('logs a rejecting clearHealth without blocking new entries from starting', async () => {
    const failure = new Error('clear-health failed')
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const rt = makeRuntime()
    rt.clearHealth.mockRejectedValueOnce(failure)
    let workers: Array<'worker-a' | 'worker-b'> = ['worker-a']
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects })
        .addDaemon('worker-a', () => makeWorker(rt.events, workers, 'worker-a'))
        .addDaemon('worker-b', () =>
          makeWorker(rt.events, workers, 'worker-b'),
        ),
    )

    const built = await reconciler.build()
    try {
      await settle()
      workers = ['worker-b']
      reconciler.scheduleRerun()
      await settle()

      expect(rt.clearHealth).toHaveBeenCalledWith({ only: ['worker-a'] })
      expect(logged).toHaveBeenCalledWith(failure)
      expect(rt.events).toContain('exec:start:worker-b')
      expect(rt.health.get('worker-b')?.result).toBe('success')
    } finally {
      await built.term()
      rt.close()
      logged.mockRestore()
    }
  })

  it('stops running entries on term without calling clearHealth, even during an in-flight removal', async () => {
    const rt = makeRuntime()
    let workers: Array<'worker-a' | 'worker-b'> = ['worker-a', 'worker-b']
    let onAbort!: () => void
    const aborted = new Promise<void>(resolve => {
      onAbort = resolve
    })
    let releaseStop!: () => void
    const stopGate = new Promise<void>(resolve => {
      releaseStop = resolve
    })
    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects })
        .addDaemon('worker-a', () => makeWorker(rt.events, workers, 'worker-a'))
        .addDaemon('worker-b', () =>
          workers.includes('worker-b')
            ? {
                subcontainer: null,
                exec: {
                  fn: (_sub: null, abort: AbortSignal) =>
                    new Promise<null>(resolve => {
                      rt.events.push('exec:start:worker-b')
                      const stop = () => {
                        onAbort()
                        stopGate.then(() => {
                          rt.events.push('exec:stop:worker-b')
                          resolve(null)
                        })
                      }
                      if (abort.aborted) return stop()
                      abort.addEventListener('abort', stop, { once: true })
                    }),
                },
                ready: ready('worker-b'),
                requires: [],
              }
            : null,
        ),
    )

    const built = await reconciler.build()
    try {
      await settle()
      expect([...rt.health.keys()].sort()).toEqual(['worker-a', 'worker-b'])

      rt.events.length = 0
      workers = ['worker-a']
      reconciler.scheduleRerun()
      await aborted

      const terming = built.term()
      releaseStop()
      await terming

      expect(rt.events).toContain('exec:stop:worker-b')
      expect(rt.events).toContain('exec:stop:worker-a')
      expect(rt.clearHealth).not.toHaveBeenCalled()
    } finally {
      releaseStop()
      await built.term()
      rt.close()
    }
  })

  it('waits for an in-flight setHealth RPC on a removed daemon before calling clearHealth', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const rt = makeRuntime()
    let workers: Array<'worker-a' | 'worker-b'> = ['worker-a', 'worker-b']
    let exitOnce: (() => void) | null = null
    let runCount = 0
    let onParkedWriteStarted!: () => void
    const parkedWriteStarted = new Promise<void>(resolve => {
      onParkedWriteStarted = resolve
    })
    let releaseParkedWrite!: () => void
    const parkedWriteGate = new Promise<void>(resolve => {
      releaseParkedWrite = resolve
    })

    const baseSetHealth = rt.setHealth.getMockImplementation()!
    rt.setHealth.mockImplementation(async params => {
      if (params.id === 'worker-b' && params.result === 'failure') {
        onParkedWriteStarted()
        await parkedWriteGate
      }
      return baseSetHealth(params)
    })

    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects })
        .addDaemon('worker-a', () => makeWorker(rt.events, workers, 'worker-a'))
        .addDaemon('worker-b', () =>
          workers.includes('worker-b')
            ? {
                subcontainer: null,
                exec: {
                  fn: (_sub: null, abort: AbortSignal) =>
                    new Promise<null>(resolve => {
                      const attempt = ++runCount
                      rt.events.push(`exec:start:worker-b:${attempt}`)
                      const stop = () => {
                        rt.events.push(`exec:stop:worker-b:${attempt}`)
                        resolve(null)
                      }
                      if (abort.aborted) return stop()
                      abort.addEventListener('abort', stop, { once: true })
                      if (attempt === 1) {
                        exitOnce = () => {
                          abort.removeEventListener('abort', stop)
                          rt.events.push('exec:unexpected-exit:worker-b')
                          resolve(null)
                        }
                      }
                    }),
                },
                ready: ready('worker-b'),
                requires: [],
              }
            : null,
        ),
    )

    const built = await reconciler.build()
    try {
      await settle()
      expect([...rt.health.keys()].sort()).toEqual(['worker-a', 'worker-b'])

      rt.events.length = 0
      exitOnce!()
      await parkedWriteStarted

      workers = ['worker-a']
      reconciler.scheduleRerun()
      await settle()

      // clearHealth must wait until the in-flight setHealth settles
      expect(rt.clearHealth).not.toHaveBeenCalled()

      releaseParkedWrite()
      await settle()

      expect(rt.clearHealth).toHaveBeenCalledTimes(1)
      expect(rt.clearHealth).toHaveBeenCalledWith({ only: ['worker-b'] })
      const setIdx = rt.events.indexOf('setHealth:worker-b:failure')
      const clearIdx = rt.events.indexOf('clearHealth:only:worker-b')
      expect(setIdx).toBeGreaterThanOrEqual(0)
      expect(setIdx).toBeLessThan(clearIdx)
      expect([...rt.health.keys()]).toEqual(['worker-a'])
      expect(rt.health.get('worker-a')?.result).toBe('success')
    } finally {
      releaseParkedWrite()
      await built.term()
      rt.close()
      logged.mockRestore()
    }
  })

  it('does not hang on term() when a removed daemon has a permanently parked setHealth RPC', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const rt = makeRuntime()
    let workers: Array<'worker-a' | 'worker-b'> = ['worker-a', 'worker-b']
    let exitOnce: (() => void) | null = null
    let runCount = 0
    let onParkedWriteStarted!: () => void
    const parkedWriteStarted = new Promise<void>(resolve => {
      onParkedWriteStarted = resolve
    })

    const baseSetHealth = rt.setHealth.getMockImplementation()!
    rt.setHealth.mockImplementation(async params => {
      if (params.id === 'worker-b' && params.result === 'failure') {
        onParkedWriteStarted()
        return new Promise<null>(() => {})
      }
      return baseSetHealth(params)
    })

    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) =>
      Daemons.of<Manifest>({ effects })
        .addDaemon('worker-a', () => makeWorker(rt.events, workers, 'worker-a'))
        .addDaemon('worker-b', () =>
          workers.includes('worker-b')
            ? {
                subcontainer: null,
                exec: {
                  fn: (_sub: null, abort: AbortSignal) =>
                    new Promise<null>(resolve => {
                      const attempt = ++runCount
                      rt.events.push(`exec:start:worker-b:${attempt}`)
                      const stop = () => {
                        rt.events.push(`exec:stop:worker-b:${attempt}`)
                        resolve(null)
                      }
                      if (abort.aborted) return stop()
                      abort.addEventListener('abort', stop, { once: true })
                      if (attempt === 1) {
                        exitOnce = () => {
                          abort.removeEventListener('abort', stop)
                          rt.events.push('exec:unexpected-exit:worker-b')
                          resolve(null)
                        }
                      }
                    }),
                },
                ready: ready('worker-b'),
                requires: [],
              }
            : null,
        ),
    )

    const built = await reconciler.build()
    try {
      await settle()
      expect([...rt.health.keys()].sort()).toEqual(['worker-a', 'worker-b'])

      rt.events.length = 0
      exitOnce!()
      await parkedWriteStarted

      workers = ['worker-a']
      reconciler.scheduleRerun()
      await settle()

      // Reconcile is now waiting on waitForHealthWrites()
      expect(rt.clearHealth).not.toHaveBeenCalled()

      // Whole-service term() must unblock reconcile and resolve promptly
      await built.term()

      expect(rt.events).toContain('exec:stop:worker-a')
      expect(rt.clearHealth).not.toHaveBeenCalled()
    } finally {
      await built.term()
      rt.close()
      logged.mockRestore()
    }
  })

  it('waits for an in-flight setHealth RPC on a restarted same-ID daemon before starting its replacement', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const rt = makeRuntime()
    let revision = 1
    let exitOnce: (() => void) | null = null
    let runCount = 0
    let onParkedWriteStarted!: () => void
    const parkedWriteStarted = new Promise<void>(resolve => {
      onParkedWriteStarted = resolve
    })
    let releaseParkedWrite!: () => void
    const parkedWriteGate = new Promise<void>(resolve => {
      releaseParkedWrite = resolve
    })

    const baseSetHealth = rt.setHealth.getMockImplementation()!
    rt.setHealth.mockImplementation(async params => {
      if (params.id === 'worker-a' && params.result === 'failure') {
        onParkedWriteStarted()
        await parkedWriteGate
      }
      return baseSetHealth(params)
    })

    const reconciler = Daemons.dynamic<Manifest>(rt.effects, ({ effects }) => {
      const rev = revision
      return Daemons.of<Manifest>({ effects }).addDaemon('worker-a', {
        subcontainer: null,
        uses: rev,
        exec: {
          fn: (_sub: null, abort: AbortSignal) =>
            new Promise<null>(resolve => {
              const attempt = ++runCount
              rt.events.push(`exec:start:worker-a:rev${rev}`)
              const stop = () => {
                rt.events.push(`exec:stop:worker-a:rev${rev}`)
                resolve(null)
              }
              if (abort.aborted) return stop()
              abort.addEventListener('abort', stop, { once: true })
              if (attempt === 1) {
                exitOnce = () => {
                  abort.removeEventListener('abort', stop)
                  rt.events.push('exec:unexpected-exit:worker-a')
                  resolve(null)
                }
              }
            }),
        },
        ready: ready('worker-a'),
        requires: [],
      })
    })

    const built = await reconciler.build()
    try {
      await settle()
      expect(rt.health.get('worker-a')?.result).toBe('success')

      rt.events.length = 0
      exitOnce!()
      await parkedWriteStarted

      revision = 2
      reconciler.scheduleRerun()
      await settle()

      // Same-ID restart keeps display, so clearHealth is not called, and the
      // replacement daemon must wait until the stopped instance's in-flight
      // setHealth finishes before starting or publishing new health.
      expect(rt.clearHealth).not.toHaveBeenCalled()
      expect(rt.events).not.toContain('exec:start:worker-a:rev2')
      expect(rt.events).not.toContain('setHealth:worker-a:starting')
      expect(rt.events).not.toContain('setHealth:worker-a:success')

      releaseParkedWrite()
      await settle()

      expect(rt.clearHealth).not.toHaveBeenCalled()
      const staleFailureIdx = rt.events.indexOf('setHealth:worker-a:failure')
      const newStartIdx = rt.events.indexOf('exec:start:worker-a:rev2')
      const newSuccessIdx = rt.events.indexOf('setHealth:worker-a:success')
      expect(staleFailureIdx).toBeGreaterThanOrEqual(0)
      expect(staleFailureIdx).toBeLessThan(newStartIdx)
      expect(newStartIdx).toBeLessThan(newSuccessIdx)
      expect(rt.health.get('worker-a')?.result).toBe('success')
    } finally {
      releaseParkedWrite()
      await built.term()
      rt.close()
      logged.mockRestore()
    }
  })

  it('handles rejecting fire-and-forget health writes without unhandled rejections or leaking pending writes', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const unhandled: unknown[] = []
    const onUnhandled = (reason: unknown) => {
      unhandled.push(reason)
    }
    process.on('unhandledRejection', onUnhandled)

    const rt = makeRuntime()
    const exitFailure = new Error('exit setHealth rejected')
    const watcherFailure = new Error('watcher setHealth rejected')
    let exitOnce: (() => void) | null = null
    let runCount = 0
    let rejectExitWrite: (err: Error) => void = () => {}
    let onExitWriteStarted!: () => void
    const exitWriteStarted = new Promise<void>(resolve => {
      onExitWriteStarted = resolve
    })

    const baseSetHealth = rt.setHealth.getMockImplementation()!
    let rejectWrites = false
    rt.setHealth.mockImplementation(async params => {
      if (
        rejectWrites &&
        params.id === 'worker-a' &&
        params.result === 'failure'
      ) {
        onExitWriteStarted()
        return new Promise<null>((_, reject) => {
          rejectExitWrite = reject
        })
      }
      if (
        rejectWrites &&
        params.id === 'worker-b' &&
        params.result === 'starting'
      ) {
        throw watcherFailure
      }
      return baseSetHealth(params)
    })

    const built = await Daemons.of<Manifest>({ effects: rt.effects })
      .addDaemon('worker-a', {
        subcontainer: null,
        exec: {
          fn: (_sub: null, abort: AbortSignal) =>
            new Promise<null>(resolve => {
              const attempt = ++runCount
              const stop = () => resolve(null)
              if (abort.aborted) return stop()
              abort.addEventListener('abort', stop, { once: true })
              if (attempt === 1) {
                exitOnce = () => {
                  abort.removeEventListener('abort', stop)
                  resolve(null)
                }
              }
            }),
        },
        ready: ready('worker-a'),
        requires: [],
      })
      .addDaemon('worker-b', {
        subcontainer: null,
        exec: blockingExec(rt.events, 'worker-b'),
        ready: ready('worker-b'),
        requires: ['worker-a'],
      })
      .build()

    try {
      await settle()
      expect(rt.health.get('worker-a')?.result).toBe('success')
      expect(rt.health.get('worker-b')?.result).toBe('success')

      const [workerA, workerB] = built.healthDaemons
      const pendingA = (
        workerA as unknown as { pendingHealthWrites: Set<Promise<void>> }
      ).pendingHealthWrites
      const pendingB = (
        workerB as unknown as { pendingHealthWrites: Set<Promise<void>> }
      ).pendingHealthWrites

      rejectWrites = true
      exitOnce!()
      await exitWriteStarted
      expect(pendingA.size).toBe(1)

      const draining = workerA.waitForHealthWrites()
      rejectExitWrite(exitFailure)
      await expect(draining).resolves.toBeUndefined()
      await expect(workerB.waitForHealthWrites()).resolves.toBeUndefined()
      await settle()

      expect(pendingA.size).toBe(0)
      expect(pendingB.size).toBe(0)
      expect(unhandled).toEqual([])
      expect(logged).toHaveBeenCalledWith(
        'Daemon worker-a exit health update failed:',
        exitFailure,
      )
      expect(logged).toHaveBeenCalledWith(
        'Daemon worker-b status update failed:',
        watcherFailure,
      )
    } finally {
      rejectExitWrite(exitFailure)
      process.off('unhandledRejection', onUnhandled)
      await built.term()
      rt.close()
      logged.mockRestore()
    }
  })
})
