import assert from 'node:assert/strict'
import { setImmediate } from 'node:timers/promises'
import { test } from 'node:test'
import { BehaviorSubject } from 'rxjs'
import { hostTime$ } from '../src/app/services/host-time.ts'

test('NTP changes re-anchor the shared host clock without polling', async t => {
  t.mock.timers.enable({ apis: ['setInterval'] })
  let elapsed = 0
  t.mock.method(performance, 'now', () => elapsed)
  const synced = new BehaviorSubject(false)
  const load = t.mock.fn(async () => ({
    now: synced.value ? '2026-10-10T12:00:00Z' : '2026-10-10T11:00:00Z',
    uptime: synced.value ? 20n : 10n,
  }))
  const time = hostTime$(synced, load)
  const values: Array<{ now: number; uptime: number; synced: boolean }> = []
  const subscription = time.subscribe(value => values.push(value))
  const second = time.subscribe()
  t.after(() => {
    subscription.unsubscribe()
    second.unsubscribe()
  })

  await setImmediate()
  t.mock.timers.tick(0)
  assert.deepEqual(values.at(-1), {
    now: Date.parse('2026-10-10T11:00:00Z'),
    uptime: 10,
    synced: false,
  })
  assert.equal(load.mock.callCount(), 1)

  elapsed = 10500
  t.mock.timers.tick(1000)
  assert.deepEqual(values.at(-1), {
    now: Date.parse('2026-10-10T11:00:10.500Z'),
    uptime: 20,
    synced: false,
  })
  synced.next(false)
  assert.equal(load.mock.callCount(), 1)

  synced.next(true)
  await setImmediate()
  t.mock.timers.tick(0)
  assert.deepEqual(values.at(-1), {
    now: Date.parse('2026-10-10T12:00:00Z'),
    uptime: 20,
    synced: true,
  })
  assert.equal(load.mock.callCount(), 2)

  elapsed = 12500
  t.mock.timers.tick(1000)
  assert.deepEqual(values.at(-1), {
    now: Date.parse('2026-10-10T12:00:02Z'),
    uptime: 22,
    synced: true,
  })
  assert.equal(load.mock.callCount(), 2)

  synced.next(false)
  await setImmediate()
  t.mock.timers.tick(0)
  assert.deepEqual(values.at(-1), {
    now: Date.parse('2026-10-10T11:00:00Z'),
    uptime: 10,
    synced: false,
  })
  assert.equal(load.mock.callCount(), 3)
})
