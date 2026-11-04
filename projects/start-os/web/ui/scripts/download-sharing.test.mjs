import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { FormControl } from '@angular/forms'
import { PatchDB } from 'patch-db-client'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export class TaskService {} export class i18nPipe {} export class DocsLinkDirective {}',
    ],
    [
      '@taiga-ui/core',
      'export class TuiButton {} export class TuiCell {} export class TuiLoader {} export class TuiNotification {} export class TuiTitle {}',
    ],
    ['@taiga-ui/kit', 'export class TuiSwitch {}'],
    [
      '@taiga-ui/layout',
      'export class TuiCardLarge {} export class TuiHeader {}',
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    ['src/app/services/title.service', 'export class TitleDirective {}'],
  ]),
)
const { default: Downloads } =
  await import('src/app/routes/portal/routes/system/routes/downloads')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService } = await import('@start9labs/shared')

function fixture(t, initial, setSeeding, enabled = async () => true) {
  const updates = new Subject()
  const patch = new PatchDB(
    updates,
    new BehaviorSubject({
      id: 1,
      value: { serverInfo: { registrySeeding: initial } },
    }),
  )
  patch.start()
  const errors = []
  let reading
  const run = async operation => {
    try {
      await operation()
      return true
    } catch (error) {
      errors.push(error)
      return false
    }
  }
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    {
      provide: ApiService,
      useValue: { rpc: { server: { bittorrent: { enabled, setSeeding } } } },
    },
    {
      provide: TaskService,
      useValue: { run, runInline: operation => (reading = run(operation)) },
    },
  ])
  t.after(() => {
    injector.destroy()
    patch.stop()
  })
  const page = runInInjectionContext(injector, () => new Downloads())
  let revision = 1
  return {
    page,
    errors,
    reading,
    observe: value =>
      updates.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/serverInfo/registrySeeding', value },
          ],
        },
      ]),
  }
}

for (const scenario of [
  { name: 'enable', initial: false, desired: true, observed: true },
  { name: 'disable', initial: true, desired: false, observed: false },
  {
    name: 'restore after failure',
    initial: false,
    desired: true,
    observed: false,
    failure: true,
  },
  {
    name: 'restore newer enabled intent after failure',
    initial: false,
    desired: true,
    observed: true,
    failure: true,
  },
  {
    name: 'restore newer disabled intent after failure',
    initial: true,
    desired: false,
    observed: false,
    failure: true,
  },
]) {
  test(`download sharing: ${scenario.name}`, async t => {
    let resolve, reject
    const remote = new Promise((yes, no) => {
      resolve = yes
      reject = no
    })
    const requests = []
    const { page, observe, errors, reading } = fixture(
      t,
      scenario.initial,
      params => {
        requests.push(params)
        return remote
      },
    )
    await reading
    assert.equal(page.seeding(), scenario.initial)
    const control = new FormControl(scenario.desired)
    control.registerOnChange((value, emitViewToModelChange) => {
      assert.equal(emitViewToModelChange, false)
    })
    const pending = page.save(scenario.desired, { control })
    assert.equal(page.saving(), true)
    observe(scenario.observed)
    if (scenario.failure) reject(new Error('save rejected'))
    else resolve(null)
    await pending
    assert.equal(page.saving(), false)
    assert.equal(page.seeding(), scenario.observed)
    assert.equal(control.value, scenario.observed)
    assert.deepEqual(requests, [{ enabled: scenario.desired }])
    assert.equal(errors.length, scenario.failure ? 1 : 0)
  })
}

test('download sharing: HTTP-only availability preserves saved intent', async t => {
  const { page, reading } = fixture(
    t,
    true,
    async () => null,
    async () => false,
  )
  assert.equal(page.availability(), 'loading')
  await reading
  assert.equal(page.availability(), false)
  assert.equal(page.seeding(), true)
})

test('download sharing: availability read failure is retryable', async t => {
  let failed = true
  const { page, reading, errors } = fixture(
    t,
    false,
    async () => null,
    async () => {
      if (failed) throw new Error('unavailable')
      return true
    },
  )
  await reading
  assert.equal(page.availability(), 'error')
  assert.equal(errors.length, 1)
  failed = false
  await page.load()
  assert.equal(page.availability(), true)
})
