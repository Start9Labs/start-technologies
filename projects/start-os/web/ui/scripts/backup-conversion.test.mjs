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
const { conversionLabel, serverBackups } =
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
    confirmations = [],
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
        openConfirm: options => {
          confirmations.push(options)
          return confirm
        },
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
    confirmations,
    tasks,
  }
}

test('format indicator distinguishes interruption, failure and active work for this server', () => {
  for (const [format, state, expected] of [
    ['current', 'complete', null],
    ['unknown', 'complete', null],
    ['legacy', 'complete', 'Update format'],
    ['interrupted', 'failed', 'Resume conversion'],
    ['legacy', 'failed', 'Retry conversion'],
    ['interrupted', 'running', 'Conversion in progress'],
  ]) {
    assert.equal(
      conversionLabel({ own: backup(format) }, { own: job(state) }, 'own'),
      expected,
    )
  }
})

test('format indicator ignores jobs for backups removed from the target', () => {
  assert.equal(
    conversionLabel(
      { own: backup('current') },
      { removed: job('running') },
      'own',
    ),
    null,
  )
  assert.equal(
    conversionLabel(
      { own: backup('legacy') },
      { removed: job('failed') },
      'own',
    ),
    'Update format',
  )
})

test('conversion indicators and dialog entries never include another server', () => {
  const backups = { own: backup('current'), other: backup('legacy') }
  for (const state of ['running', 'failed']) {
    const jobs = { other: job(state) }
    assert.equal(conversionLabel(backups, jobs, 'own'), null)
    assert.equal(
      conversionLabel({ own: backup('legacy') }, jobs, 'own'),
      'Update format',
    )
  }
  assert.deepEqual(serverBackups(backups, 'own'), { own: backups.own })
  assert.deepEqual(serverBackups(backups, 'absent'), {})
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
    conversionLabel(
      refreshed.entry.startOs,
      f.service.conversions().target,
      'own',
    ),
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

test('backup creation offers an update and skip; skipping starts no conversion', async t => {
  const f = fixture(t, { confirm: of(false) })
  assert.equal(
    await f.service.prepareBackup('target', { own: backup('legacy') }),
    'ready',
  )
  assert.equal(f.confirmations[0].data.yes, 'Update format')
  assert.equal(f.confirmations[0].data.no, 'Skip')
  assert.deepEqual(f.submissions, [])
  assert.deepEqual(f.prompts, [])
})

test('choosing update starts this server conversion rather than a backup', async t => {
  const f = fixture(t)
  assert.equal(
    await f.service.prepareBackup('target', {
      own: backup('legacy'),
      other: backup('legacy'),
    }),
    'conversion',
  )
  assert.deepEqual(f.submissions, [
    { targetId: 'target', serverId: 'own', password: 'secret' },
  ])
})

test('dismissing either conversion prompt cancels backup creation', async t => {
  const f = fixture(t, { confirm: EMPTY })
  assert.equal(
    await f.service.prepareBackup('target', { own: backup('legacy') }),
    'cancelled',
  )
  const g = fixture(t, { password: EMPTY })
  assert.equal(
    await g.service.prepareBackup('target', { own: backup('legacy') }),
    'cancelled',
  )
  assert.deepEqual(f.submissions, [])
  assert.deepEqual(g.submissions, [])
})

test('other-server backups do not prompt; interrupted or running own backups open conversion', async t => {
  const f = fixture(t)
  for (const backups of [
    { other: backup('legacy') },
    { own: backup('current') },
    {},
  ]) {
    assert.equal(await f.service.prepareBackup('target', backups), 'ready')
  }
  assert.equal(
    await f.service.prepareBackup('target', { own: backup('interrupted') }),
    'conversion',
  )
  f.observations.next({ target: { own: job('running') } })
  assert.equal(
    await f.service.prepareBackup('target', { own: backup('legacy') }),
    'conversion',
  )
  assert.deepEqual(f.confirmations, [])
  assert.deepEqual(f.submissions, [])
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
