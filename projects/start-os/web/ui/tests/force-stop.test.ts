import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { T } from '@start9labs/start-core'
import { BehaviorSubject, EMPTY, of, Subject } from 'rxjs'
import {
  confirmForceStop,
  forceStopAt,
  forceStopAt$,
} from '../src/app/services/force-stop.ts'

const deadline = '2026-10-10T12:00:30.123456Z'
const stopped: T.StatusInfo = {
  desired: { main: 'stopped' },
  started: '2026-10-10T11:00:00Z',
  forceStopAt: deadline,
  error: null,
  health: {},
}

test('the mock eligibility predicate rejects early, completed and stale stop episodes', () => {
  assert.equal(forceStopAt(stopped, Date.parse(deadline) - 1), null)
  assert.equal(
    forceStopAt({ ...stopped, started: null }, Date.parse(deadline)),
    null,
  )
  assert.equal(
    forceStopAt({ ...stopped, forceStopAt: null }, Date.parse(deadline)),
    null,
  )
  assert.equal(
    forceStopAt(
      { ...stopped, desired: { main: 'running' } },
      Date.parse(deadline),
    ),
    null,
  )
  const nextDeadline = '2026-10-10T12:05:30.123456Z'
  const eligibleToken = forceStopAt(
    { ...stopped, forceStopAt: nextDeadline },
    Date.parse(nextDeadline),
  )
  assert.equal(eligibleToken, nextDeadline)
  assert.notEqual(eligibleToken, deadline)
  assert.equal(forceStopAt(stopped, Date.parse(deadline)), deadline)
})

test('host time crossing the deadline enables force stop without a status update', () => {
  const now = new BehaviorSubject({ now: Date.parse(deadline) - 1 })
  const values: Array<string | null> = []
  const subscription = forceStopAt$(of(stopped), now).subscribe(value =>
    values.push(value),
  )

  assert.deepEqual(values, [null])
  now.next({ now: Date.parse(deadline) })
  assert.deepEqual(values, [null, deadline])
  now.next({ now: Date.parse(deadline) + 60000 })
  assert.deepEqual(values, [null, deadline])
  subscription.unsubscribe()
})

test('eligibility follows raw stop intent, availability and the server token', () => {
  const status = new BehaviorSubject(stopped)
  const values: Array<string | null> = []
  const subscription = forceStopAt$(
    status,
    of({ now: Date.parse(deadline) + 1 }),
  ).subscribe(value => values.push(value))

  status.next({ ...stopped, desired: { main: 'running' } })
  status.next(stopped)
  status.next({ ...stopped, started: null })
  status.next(stopped)
  status.next({ ...stopped, forceStopAt: null })
  assert.deepEqual(values, [deadline, null, deadline, null, deadline, null])
  subscription.unsubscribe()
})

test('a new stop episode must wait for its own deadline', () => {
  const status = new BehaviorSubject(stopped)
  const now = new BehaviorSubject({ now: Date.parse(deadline) + 1 })
  const nextDeadline = '2026-10-10T12:05:30Z'
  const values: Array<string | null> = []
  const subscription = forceStopAt$(status, now).subscribe(value =>
    values.push(value),
  )

  status.next({ ...stopped, forceStopAt: nextDeadline })
  assert.deepEqual(values, [deadline, null])
  now.next({ now: Date.parse(nextDeadline) })
  assert.deepEqual(values, [deadline, null, nextDeadline])
  subscription.unsubscribe()
})

test('an error does not hide a raw eligible stop episode', () => {
  const values: Array<string | null> = []
  forceStopAt$(
    of({
      ...stopped,
      error: { details: 'runtime unresponsive', debug: 'timeout', info: null },
    }),
    of({ now: Date.parse(deadline) }),
  ).subscribe(value => values.push(value))
  assert.deepEqual(values, [deadline])
})

test('confirmation captures the exact token before opening the dialog', async () => {
  const params: T.ForceStopParams = { id: 'electrs', forceStopAt: deadline }
  const confirmation = new Subject<boolean>()
  const requests: T.ForceStopParams[] = []
  const pending = confirmForceStop(
    params,
    () => {
      params.forceStopAt = '2026-10-10T12:05:30Z'
      return confirmation
    },
    async request => {
      requests.push(request)
    },
  )

  assert.deepEqual(requests, [])
  confirmation.next(true)
  await pending
  assert.deepEqual(requests, [{ id: 'electrs', forceStopAt: deadline }])
})

test('cancellation and dismissal send no request', async () => {
  const requests: T.ForceStopParams[] = []
  for (const confirmation of [of(false), EMPTY]) {
    await confirmForceStop(
      { id: 'electrs', forceStopAt: deadline },
      () => confirmation,
      async request => {
        requests.push(request)
      },
    )
  }
  assert.deepEqual(requests, [])
})
