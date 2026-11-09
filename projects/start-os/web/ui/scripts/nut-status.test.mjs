import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export const getErrorMessage = error => error.message ?? String(error)',
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
  ]),
)
const { PatchDB } = await import('patch-db-client')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { NutStatusService } =
  await import('src/app/routes/portal/routes/system/routes/nut/service')

const enabled = {
  enabled: true,
  settings: { mode: 'client', host: 'nut.test' },
}
const healthy = {
  target: 'ups@nut.test:3493',
  variables: { 'ups.status': 'OL' },
  monitoringError: null,
}
const flush = async () => {
  await Promise.resolve()
  await Promise.resolve()
}

function fixture(t) {
  t.mock.timers.enable({ apis: ['setTimeout', 'setInterval'] })
  const config = new BehaviorSubject(enabled)
  const requests = []
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: { watch$: () => config } },
    {
      provide: ApiService,
      useValue: {
        rpc: {
          server: {
            nutStatus: () =>
              new Promise((resolve, reject) =>
                requests.push({ resolve, reject }),
              ),
          },
        },
      },
    },
  ])
  const service = runInInjectionContext(injector, () => new NutStatusService())
  t.after(() => {
    if (!injector.destroyed) injector.destroy()
  })
  t.mock.timers.tick(0)
  return { config, requests, service, injector }
}

test('periodic checks retain readings while loading and show authentication failure and recovery', async t => {
  const { requests, service } = fixture(t)
  assert.equal(requests.length, 1)
  requests[0].resolve(healthy)
  await flush()
  assert.deepEqual(service.state(), {
    status: healthy,
    error: null,
    refreshing: false,
  })
  t.mock.timers.tick(15000)
  assert.equal(requests.length, 2)
  assert.equal(service.state().status, healthy)
  assert.equal(service.state().refreshing, true)
  const denied = { ...healthy, monitoringError: 'Monitoring login rejected' }
  requests[1].resolve(denied)
  await flush()
  assert.equal(service.state().status.monitoringError, denied.monitoringError)
  t.mock.timers.tick(15000)
  requests[2].resolve(healthy)
  await flush()
  assert.equal(service.state().status.monitoringError, null)
})

test('network errors remain visible until a successful recheck, without overlapping requests', async t => {
  const { requests, service } = fixture(t)
  service.refresh()
  t.mock.timers.tick(15000)
  assert.equal(requests.length, 1)
  requests[0].reject({ message: 'UPS unreachable' })
  await flush()
  assert.deepEqual(service.state(), {
    status: null,
    error: 'UPS unreachable',
    refreshing: false,
  })
  service.refresh()
  assert.equal(requests.length, 2)
  assert.equal(service.state().error, 'UPS unreachable')
  requests[1].resolve(healthy)
  await flush()
  assert.equal(service.state().error, null)
})

test('configuration changes ignore stale responses and disabling stops checks', async t => {
  const { config, requests, service } = fixture(t)
  config.next({
    ...enabled,
    settings: { ...enabled.settings, host: 'other.test' },
  })
  t.mock.timers.tick(0)
  assert.equal(requests.length, 2)
  requests[1].resolve(healthy)
  await flush()
  requests[0].resolve({ ...healthy, monitoringError: 'Stale failure' })
  await flush()
  assert.equal(service.state().status.monitoringError, null)
  config.next({ enabled: false, settings: enabled.settings })
  assert.deepEqual(service.state(), {
    status: null,
    error: null,
    refreshing: false,
  })
  t.mock.timers.tick(30000)
  service.refresh()
  assert.equal(requests.length, 2)
})

test('destroying the page cancels polling and ignores pending responses', async t => {
  const { requests, service, injector } = fixture(t)
  injector.destroy()
  requests[0].resolve(healthy)
  await flush()
  assert.equal(service.state().status, null)
  t.mock.timers.tick(30000)
  assert.equal(requests.length, 1)
})
