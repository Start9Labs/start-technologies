import assert from 'node:assert/strict'
import test from 'node:test'
import { setImmediate } from 'node:timers/promises'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject, EMPTY, of } from 'rxjs'
import { PatchDB } from 'patch-db-client'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `
    export class DialogService {}
    export class ErrorService {}
    export class TaskService {}
    export const getErrorMessage = error => error.message;
  `,
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
  ]),
)
const { DialogService, ErrorService, TaskService } =
  await import('@start9labs/shared')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { BackupService } =
  await import('../src/app/routes/portal/routes/system/routes/backups/backup.service.ts')
const { conversionLabel } =
  await import('../src/app/routes/portal/routes/system/routes/backups/conversion-state.ts')

const backup = formatStatus => ({
  hostname: 'server',
  timestamp: null,
  version: '0.4.0',
  formatStatus,
})
const target = formatStatus => ({
  type: 'cifs',
  hostname: 'host',
  path: '/backups',
  username: '',
  mountable: true,
  available: 100,
  startOs: { own: backup(formatStatus) },
  legacyBackup: false,
})
const job = state => ({
  state,
  progress: { overall: state === 'complete', phases: [] },
  error: null,
})

function fixture(t, { confirm = of(true), password = of('secret') } = {}) {
  const observations = new BehaviorSubject({})
  const requests = [],
    submissions = [],
    errors = [],
    prompts = [],
    tasks = []
  const api = {
    rpc: {
      backup: {
        target: {
          list: () =>
            new Promise((resolve, reject) =>
              requests.push({ resolve, reject }),
            ),
          convert: async params => {
            submissions.push(params)
            return null
          },
        },
      },
    },
  }
  const injector = createEnvironmentInjector([
    { provide: ApiService, useValue: api },
    {
      provide: PatchDB,
      useValue: {
        watch$: (...path) => (path.at(-1) === 'id' ? of('own') : observations),
      },
    },
    {
      provide: ErrorService,
      useValue: { handleError: error => errors.push(error) },
    },
    {
      provide: DialogService,
      useValue: {
        openConfirm: () => confirm,
        openPrompt: options => {
          prompts.push(options)
          return password
        },
      },
    },
    {
      provide: TaskService,
      useValue: {
        run: async (fn, content) => {
          tasks.push(content)
          await fn()
          return true
        },
      },
    },
  ])
  const service = runInInjectionContext(injector, () => new BackupService())
  t.after(() => injector.destroy())
  return {
    service,
    observations,
    requests,
    submissions,
    errors,
    prompts,
    tasks,
  }
}

test('format indicator distinguishes interruption, failure and active work across server backups', () => {
  assert.equal(
    conversionLabel({ a: backup('current'), b: backup('unknown') }),
    null,
  )
  assert.equal(conversionLabel({ a: backup('legacy') }), 'Old format')
  assert.equal(
    conversionLabel({ a: backup('legacy'), b: backup('interrupted') }),
    'Resume conversion',
  )
  assert.equal(
    conversionLabel({ a: backup('legacy') }, { a: job('failed') }),
    'Retry conversion',
  )
  assert.equal(
    conversionLabel(
      { a: backup('interrupted'), b: backup('current') },
      { b: job('running') },
    ),
    'Conversion in progress',
  )
  assert.equal(
    conversionLabel({ a: backup('current') }, { a: job('complete') }),
    null,
  )
})

test('format indicator ignores jobs for backups removed from the target', () => {
  assert.equal(
    conversionLabel({ a: backup('current') }, { removed: job('running') }),
    null,
  )
  assert.equal(
    conversionLabel({ a: backup('legacy') }, { removed: job('failed') }),
    'Old format',
  )
})

test('legacy discovery preserves backup/restore eligibility and never starts conversion', async t => {
  const f = fixture(t)
  f.service.getBackupTargets()
  f.requests[0].resolve({ legacy: target('legacy') })
  await setImmediate()
  assert.equal(f.service.cifs()[0].hasAnyBackup, true)
  assert.equal(f.service.cifs()[0].hasCurrentBackup, true)
  assert.deepEqual(f.submissions, [])
  assert.equal(f.service.loading(), false)
})

test('a late discovery response cannot replace a newer target view; failures retain the view', async t => {
  const f = fixture(t)
  f.service.getBackupTargets()
  f.service.getBackupTargets()
  f.requests[1].resolve({ latest: target('current') })
  await setImmediate()
  f.requests[0].resolve({ stale: target('legacy') })
  await setImmediate()
  assert.equal(f.service.cifs()[0].id, 'latest')
  f.service.getBackupTargets()
  f.requests[2].reject(new Error('offline'))
  await setImmediate()
  assert.equal(f.service.cifs()[0].id, 'latest')
  assert.deepEqual(f.errors, ['offline'])
  assert.equal(f.service.loading(), false)
})

test('completion refreshes discovery once, not on every progress observation', async t => {
  const f = fixture(t)
  f.observations.next({ target: { own: job('running') } })
  f.observations.next({ target: { own: job('running') } })
  assert.equal(f.requests.length, 0)
  f.observations.next({ target: { own: job('complete') } })
  assert.equal(f.requests.length, 1)
  f.observations.next({ target: { own: job('complete') } })
  assert.equal(f.requests.length, 1)
  f.requests[0].resolve({ target: target('current') })
  await setImmediate()
  assert.equal(f.service.cifs()[0].entry.startOs.own.formatStatus, 'current')
})

test('failure refreshes discovery once and exposes the interrupted format', async t => {
  const f = fixture(t)
  f.service.getBackupTargets()
  f.requests[0].resolve({ target: target('legacy') })
  await setImmediate()
  f.observations.next({ target: { own: job('running') } })
  f.observations.next({ target: { own: job('failed') } })
  assert.equal(f.requests.length, 2)
  f.observations.next({ target: { own: job('failed') } })
  assert.equal(f.requests.length, 2)
  f.requests[1].resolve({ target: target('interrupted') })
  await setImmediate()
  const refreshed = f.service.cifs()[0]
  assert.equal(refreshed.entry.startOs.own.formatStatus, 'interrupted')
  assert.equal(
    conversionLabel(refreshed.entry.startOs, f.service.conversions().target),
    'Resume conversion',
  )
})

test('coalesced terminal observations refresh without requiring a visible running state', async t => {
  for (const state of ['complete', 'failed']) {
    const f = fixture(t)
    f.observations.next({ target: { own: job(state) } })
    assert.equal(f.requests.length, 1)
    f.requests[0].resolve({
      target: target(state === 'complete' ? 'current' : 'interrupted'),
    })
    await setImmediate()
  }
})

test('conversion requires explicit confirmation and a password, scoped to one server', async t => {
  const f = fixture(t)
  await f.service.convert('disk-target', 'archived-server')
  assert.deepEqual(f.submissions, [
    {
      targetId: 'disk-target',
      serverId: 'archived-server',
      password: 'secret',
    },
  ])
  assert.equal(f.prompts.length, 1)
  assert.deepEqual(f.tasks, ['Starting backup conversion'])
})

test('declined or dismissed confirmation never prompts for a credential', async t => {
  for (const confirm of [of(false), EMPTY]) {
    const f = fixture(t, { confirm })
    await f.service.convert('target', 'own')
    assert.deepEqual(f.prompts, [])
    assert.deepEqual(f.submissions, [])
  }
})

test('dismissing the password prompt starts no background job', async t => {
  const f = fixture(t, { password: EMPTY })
  await f.service.convert('target', 'own')
  assert.deepEqual(f.submissions, [])
})
