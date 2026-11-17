import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject, firstValueFrom } from 'rxjs'
import { PatchDB } from 'patch-db-client'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
  ]),
)
const { mergeBackupHistory, readManualHistory } =
  await import('src/app/routes/portal/routes/backups/history-data')
const { BackupHistoryService } =
  await import('src/app/routes/portal/routes/backups/history.service')
const { ApiService } = await import('src/app/services/api/embassy-api.service')

const tick = () => new Promise(resolve => setImmediate(resolve))
const at = ns => `2026-11-16T00:00:00.${String(ns).padStart(9, '0')}Z`
const report = (error = null) => ({
  error,
  duration_ms: 1,
  logical_size: null,
  physical_size: null,
  changed_bytes: null,
  measured_at: null,
})
const activity = (id, start, end, kind = 'manual') => ({
  id,
  kind,
  targetId: 'nas',
  sourceServerId: null,
  jobId: null,
  jobName: null,
  trigger: null,
  state: end ? 'succeeded' : 'running',
  startedAt: at(start),
  completedAt: end ? at(end) : null,
  intendedServices: ['service'],
  services: { service: report() },
  error: null,
})
const run = (id, start = 150, end = 250) => ({
  id,
  targetId: 'nas',
  targetName: 'Original NAS name',
  startedAt: at(start),
  completedAt: at(end),
  report: {
    server: { attempted: true, error: null },
    packages: { service: report() },
    sharedVolumes: { photos: { name: 'Photos', ...report() } },
  },
})

test('a manual backup appears once with its full original report; input activity is unchanged', () => {
  const a = activity('activity', 100, 300)
  const r = run('report')
  const merged = mergeBackupHistory([a], [r])
  assert.equal(merged.length, 1)
  assert.equal(merged[0].id, a.id)
  assert.equal(merged[0].manualRun, r)
  assert.equal(merged[0].manualRun.report.sharedVolumes.photos.name, 'Photos')
  assert.equal(a.manualRun, undefined)
})

test('matching keeps adjacent nanosecond intervals distinct and never attaches to automatic or restore activity', () => {
  const first = activity('first', 100, 400)
  const second = activity('second', 500, 900)
  const older = run('older', 150, 350)
  const newer = run('newer', 550, 850)
  const merged = mergeBackupHistory(
    [
      first,
      second,
      activity('automatic', 100, 900, 'automatic'),
      activity('restore', 100, 900, 'restore'),
    ],
    [older, newer],
  )
  assert.equal(merged.length, 4)
  assert.equal(merged.find(a => a.id === 'first').manualRun.id, 'older')
  assert.equal(merged.find(a => a.id === 'second').manualRun.id, 'newer')
  assert.equal(merged.find(a => a.kind === 'automatic').manualRun, undefined)
  assert.equal(merged.find(a => a.kind === 'restore').manualRun, undefined)
})

test('reports from another target or outside an activity interval remain separate entries', () => {
  const other = { ...run('other'), targetId: 'removed-target' }
  const outside = run('outside', 450, 550)
  const entries = mergeBackupHistory(
    [activity('manual', 100, 300)],
    [other, outside],
  )
  assert.equal(entries.length, 3)
  assert.equal(entries[0].manualRun, outside)
  assert.equal(
    entries.find(a => a.targetId === 'removed-target').manualRun.targetName,
    'Original NAS name',
  )
})

test('older reports survive activity retention and include shared-volume failures in their status', () => {
  const retained = Array.from({ length: 1000 }, (_, i) =>
    activity(`auto-${i}`, i + 1000, i + 1001, 'automatic'),
  )
  const partial = run('old-partial')
  partial.report.sharedVolumes.photos.error = 'volume unavailable'
  const failed = run('old-failed', 10, 20)
  failed.report.server.error = 'target lost'
  failed.report.packages.service.error = 'target lost'
  failed.report.sharedVolumes.photos.error = 'target lost'
  const entries = mergeBackupHistory(retained, [partial, failed])
  assert.equal(entries.length, 1002)
  assert.equal(
    entries.find(a => a.manualRun?.id === partial.id).state,
    'partiallyFailed',
  )
  assert.equal(entries.at(-1).state, 'failed')
})

test('manual report pagination reads every page instead of losing records beyond the first page', async () => {
  const runs = Array.from({ length: 405 }, (_, i) => run(`report-${i}`))
  const calls = []
  const result = await firstValueFrom(
    readManualHistory(async params => {
      calls.push(params)
      return {
        total: runs.length,
        runs: runs.slice(params.offset, params.offset + params.limit),
      }
    }),
  )
  assert.deepEqual(result, runs)
  assert.deepEqual(calls, [
    { offset: 0, limit: 200 },
    { offset: 200, limit: 200 },
    { offset: 400, limit: 200 },
  ])
})

test('canceling a history read stops pagination after the outstanding request', async () => {
  let finish
  let requests = 0
  const subscription = readManualHistory(() => {
    requests++
    return new Promise(resolve => {
      finish = resolve
    })
  }).subscribe()
  subscription.unsubscribe()
  finish({ runs: [run('first')], total: 2 })
  await tick()
  assert.equal(requests, 1)
})

test('an incomplete history response fails instead of silently hiding records', async () => {
  await assert.rejects(
    firstValueFrom(readManualHistory(async () => ({ runs: [], total: 1 }))),
    /Incomplete backup history/,
  )
})

test('history refresh retains reports on failure and retry ignores superseded responses', async t => {
  const activities = new BehaviorSubject({})
  const requests = []
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: { watch$: () => activities } },
    {
      provide: ApiService,
      useValue: {
        rpc: {
          backup: {
            history: {
              list: () =>
                new Promise((resolve, reject) =>
                  requests.push({ resolve, reject }),
                ),
            },
          },
        },
      },
    },
  ])
  t.after(() => injector.destroy())
  const model = runInInjectionContext(
    injector,
    () => new BackupHistoryService(),
  )
  assert.equal(model.reports().loading, true)
  requests[0].resolve({ runs: [run('first')], total: 1 })
  await tick()
  assert.equal(model.entries().length, 1)
  model.retry()
  requests[1].reject(new Error('offline'))
  await tick()
  assert.equal(model.reports().error, true)
  assert.equal(model.entries()[0].manualRun.id, 'first')
  model.retry()
  model.retry()
  requests[3].resolve({ runs: [run('latest')], total: 1 })
  await tick()
  requests[2].resolve({ runs: [run('stale')], total: 1 })
  await tick()
  assert.equal(model.entries()[0].manualRun.id, 'latest')
  assert.equal(model.reports().error, false)
  assert.equal(model.reports().loading, false)
  activities.next({ running: activity('running', 600, null) })
  requests[4].resolve({ runs: [run('latest')], total: 1 })
  await tick()
  activities.next({ running: activity('running', 600, 900) })
  assert.equal(
    requests.length,
    6,
    'completed manual activity refreshes its detailed report',
  )
  requests[5].resolve({
    runs: [run('latest'), run('completed', 650, 850)],
    total: 2,
  })
  await tick()
  assert.equal(model.entries().length, 2)
  assert.equal(
    model.entries().find(entry => entry.id === 'running').manualRun.id,
    'completed',
  )
})
