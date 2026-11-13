import assert from 'node:assert/strict'
import test from 'node:test'
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
  name: 'Target',
  source: { kind: 'cifs', hostname: 'host', path: '/backups' },
  availability: 'available',
  available: 100,
  error: null,
  checkedAt: null,
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
  const targets = new BehaviorSubject({})
  const submissions = [],
    prompts = [],
    confirmations = [],
    tasks = []
  const api = {
    rpc: {
      backup: {
        target: {
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
        watch$: (...path) =>
          path.at(-1) === 'id'
            ? of('own')
            : path[0] === 'backupTargets'
              ? targets
              : observations,
      },
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
    { provide: ErrorService, useValue: { handleError: () => {} } },
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
    targets,
    submissions,
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

test('targets come from the database and keep backup/restore eligibility without starting conversion', t => {
  const f = fixture(t)
  f.targets.next({ legacy: target('legacy') })
  const entry = f.service.targets().legacy
  assert.equal(f.service.hasAnyBackup(entry), true)
  assert.equal(f.service.hasCurrentBackup(entry), true)
  assert.deepEqual(f.submissions, [])
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

test('conversion asks only for the password, scoped to one server', async t => {
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
  assert.deepEqual(f.confirmations, [])
  assert.deepEqual(f.tasks, ['Starting backup conversion'])
})

test('dismissing the password prompt starts no background job', async t => {
  const f = fixture(t, { password: EMPTY })
  await f.service.convert('target', 'own')
  assert.deepEqual(f.submissions, [])
})
