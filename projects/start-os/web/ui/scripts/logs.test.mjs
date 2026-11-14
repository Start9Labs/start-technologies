import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Injector } from '@angular/core'
import { BehaviorSubject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'
const shared = new URL(
  '../../../../../shared-libs/ts-modules/shared/src/',
  import.meta.url,
)
loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `export { convertAnsi } from '${new URL('util/convert-ansi.ts', shared)}'; export { toLocalIsoString } from '${new URL('util/to-local-iso-string.ts', shared)}'; export { withAbortSignal } from '${new URL('util/with-abort.ts', shared)}'; export class i18nPipe {}`,
    ],
    ['./logs.component', 'export class LogsComponent {}'],
    [
      'src/app/services/connection.service',
      'export class ConnectionService {}',
    ],
  ]),
)
const { LogsPipe } =
  await import('../src/app/routes/portal/components/logs/logs.pipe.ts')
const { LogsComponent } = await import('./logs.component')
const { ConnectionService } =
  await import('src/app/services/connection.service')
const { i18nPipe } = await import('@start9labs/shared')
const { pauseFor } = await import(new URL('util/misc.util.ts', shared))
const flush = () => new Promise(setImmediate)
const entry = message => ({
  timestamp: '2026-11-12T00:00:00Z',
  bootId: 'boot',
  message,
})
const reply = items => ({
  initial: { startCursor: 'cursor', guid: null },
  items,
})
function fixture(t, follow, available = false) {
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'] })
  const logs = {
    status$: new BehaviorSubject('connected'),
    setCursor(value) {
      this.startCursor = value
    },
  }
  const connection = new BehaviorSubject(available)
  const injector = Injector.create({
    providers: [
      LogsPipe,
      { provide: LogsComponent, useValue: logs },
      { provide: ConnectionService, useValue: connection },
      { provide: i18nPipe, useValue: { transform: value => value } },
    ],
  })
  const values = [],
    subscription = injector
      .get(LogsPipe)
      .transform(follow)
      .subscribe(value => values.push(value))
  t.after(() => subscription.unsubscribe())
  return { logs, connection, values, subscription }
}

test('portal log acquisition disposal cannot adopt a late reply or start recovery', async t => {
  const pending = Promise.withResolvers()
  let signal,
    started = 0,
    calls = 0
  const f = fixture(t, (params, options) => {
    assert.equal(params.limit, 400)
    calls++
    signal = options.signal
    return pending.promise
  })
  f.subscription.unsubscribe()
  pending.resolve(
    reply(
      (async function* () {
        started++
        yield entry('late')
        return null
      })(),
    ),
  )
  await flush()
  t.mock.timers.tick(10000)
  await flush()
  assert.equal(signal.aborted, true)
  assert.equal(started, 0)
  assert.equal(calls, 1)
  assert.deepEqual(f.values, [])
})

test('portal logs preserve cursor, ordered one-second batches and active iterator disposal', async t => {
  let signal,
    disposed = 0
  const f = fixture(t, async (_, options) => {
    signal = options.signal
    return reply(
      (async function* () {
        try {
          yield entry('first')
          yield entry('second')
          await pauseFor(60000, signal)
          return null
        } finally {
          disposed++
        }
      })(),
    )
  })
  await flush()
  assert.equal(f.logs.startCursor, 'cursor')
  t.mock.timers.tick(999)
  assert.deepEqual(f.values, [])
  t.mock.timers.tick(1)
  assert.equal(f.values.length, 1)
  assert.ok(f.values[0][0].indexOf('first') < f.values[0][0].indexOf('second'))
  f.subscription.unsubscribe()
  await flush()
  assert.equal(signal.aborted, true)
  assert.equal(disposed, 1)
})

for (const outcome of ['normal terminal', 'transport failure'])
  test(`portal ${outcome} enters existing availability recovery without restarting a disposed view`, async t => {
    let calls = 0
    const f = fixture(t, async () => {
      calls++
      return reply(
        (async function* () {
          if (outcome === 'transport failure')
            throw Object.assign(new Error('lost'), { code: 0 })
          return null
        })(),
      )
    })
    await flush()
    assert.equal(f.logs.status$.value, 'disconnected')
    assert.match(f.values.at(-1).join(''), /Disconnected/)
    f.connection.next(true)
    assert.equal(f.logs.status$.value, 'reconnecting')
    t.mock.timers.tick(999)
    await flush()
    assert.equal(calls, 1)
    f.subscription.unsubscribe()
    t.mock.timers.tick(10000)
    await flush()
    assert.equal(calls, 1)
  })

test('portal recovery retains history/status banners and read-only reconnect options', async t => {
  const requests = []
  const f = fixture(
    t,
    async (params, options) => {
      requests.push({ params, signal: options.signal })
      return reply(
        (async function* () {
          yield entry(requests.length === 1 ? 'before loss' : 'after recovery')
          if (requests.length === 1) return null
          await pauseFor(60000, options.signal)
          return null
        })(),
      )
    },
    true,
  )
  await flush()
  assert.equal(requests[0].params.limit, 400)
  t.mock.timers.tick(1000)
  await flush()
  assert.equal(requests.length, 2)
  assert.deepEqual(requests[1].params, {})
  assert.notEqual(requests[0].signal, requests[1].signal)
  assert.equal(requests[0].signal.aborted, true)
  assert.equal(f.logs.status$.value, 'connected')
  t.mock.timers.tick(1000)
  await flush()
  const html = f.values.at(-1).join('')
  assert.match(html, /before loss/)
  assert.match(html, /Disconnected/)
  assert.match(html, /Reconnected/)
  assert.match(html, /after recovery/)
})
