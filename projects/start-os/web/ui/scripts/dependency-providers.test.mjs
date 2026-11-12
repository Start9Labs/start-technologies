import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
} from '@angular/core'
import { BehaviorSubject, firstValueFrom, of } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      'src/app/routes/portal/components/pkg-icon',
      'export class PkgIconDirective {};',
    ],
    [
      'src/app/routes/portal/components/placeholder.component',
      'export class PlaceholderComponent {};',
    ],
    ['patch-db-client', 'export class PatchDB {}'],
    [
      '@start9labs/shared',
      'export class DialogService {}; export class TaskService {}; export class i18nPipe {};',
    ],
    [
      '@taiga-ui/core',
      'export class TuiButton {}; export class TuiCell {}; export class TuiDataList {}; export class TuiIcon {}; export class TuiTextfield {}; export class TuiTitle {};',
    ],
    [
      '@taiga-ui/kit',
      'export class TuiAvatar {}; export class TuiFade {}; export class TuiChevron {}; export class TuiSelect {}; export class TuiSkeleton {};',
    ],
    ['src/app/services/action.service', 'export class ActionService {};'],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {};'],
    [
      'src/app/services/package-icon.service',
      'export class PackageIconService {};',
    ],
    [
      'src/app/routes/portal/routes/services/components/tasks.component',
      'export class ServiceTasksComponent {};',
    ],
  ]),
)
const { dependencyProvider } =
  await import('../src/app/utils/dependency-provider.ts')
const { hasCurrentDeps } = await import('../src/app/utils/has-deps.ts')
const { getLiveTasks } = await import('../src/app/utils/get-package-data.ts')
const { dryUpdate } = await import('../src/app/utils/dry-update.ts')
const { DepErrorService } =
  await import('../src/app/services/dep-error.service.ts')
const { ServiceTaskComponent } =
  await import('../src/app/routes/portal/routes/services/components/task.component.ts')
const { PatchDB } = await import('patch-db-client')
const { DialogService, TaskService, i18nPipe } =
  await import('@start9labs/shared')
const { ActionService } = await import('src/app/services/action.service')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { PackageIconService } =
  await import('src/app/services/package-icon.service')
const { ServiceTasksComponent } =
  await import('src/app/routes/portal/routes/services/components/tasks.component')

function pkg(id, overrides = {}) {
  return {
    stateInfo: {
      state: 'installed',
      manifest: { id, title: id, version: '1.0.0' },
    },
    currentDependencies: {},
    dependencySelections: {},
    dependencyBindings: {},
    tasks: {},
    actions: {},
    statusInfo: {
      desired: { main: 'running' },
      started: 'now',
      health: {},
      error: null,
    },
    ...overrides,
  }
}
function dependent(id = 'consumer', providerId = 'alternate') {
  return pkg(id, {
    currentDependencies: {
      bitcoin: {
        kind: 'running',
        versionRange: '>=26.0.0',
        healthChecks: ['rpc'],
      },
    },
    dependencyBindings: {
      bitcoin: {
        providerId,
        versions: providerId ? ['26.0.0'] : [],
        candidates: { bitcoin: ['26.0.0'], alternate: ['26.0.0'] },
      },
    },
  })
}
function errors(t, packages) {
  const source = new BehaviorSubject(packages)
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: { watch$: () => source } },
  ])
  t.after(() => injector.destroy())
  const service = runInInjectionContext(injector, () => new DepErrorService())
  return { source, read: () => firstValueFrom(service.depErrors$) }
}

test('dependency targets use only the authoritative owner binding; self and admin stay physical', () => {
  const owner = dependent()
  assert.equal(dependencyProvider(owner, 'bitcoin'), 'alternate')
  assert.equal(dependencyProvider(owner, 'consumer'), 'consumer')
  owner.currentDependencies['start-os'] = { kind: 'exists' }
  assert.equal(dependencyProvider(owner, 'start-os'), 'start-os')
  assert.equal(dependencyProvider(owner, 'unrelated'), 'unrelated')
  owner.dependencyBindings.bitcoin.providerId = null
  assert.equal(dependencyProvider(owner, 'bitcoin'), null)
  delete owner.dependencyBindings
  assert.equal(dependencyProvider(owner, 'bitcoin'), null)
})

test('stop warnings include alias consumers but not unselected candidates', () => {
  const packages = { consumer: dependent() }
  assert.equal(hasCurrentDeps('alternate', packages), true)
  assert.equal(hasCurrentDeps('bitcoin', packages), false)
  packages.consumer.dependencyBindings.bitcoin.providerId = null
  assert.equal(hasCurrentDeps('alternate', packages), false)
})

test('unresolved logical tasks stay visible without rewriting their target intent', () => {
  const owner = dependent('consumer', null)
  const entry = target => ({ active: true, task: { packageId: target } })
  owner.tasks = {
    logical: entry('bitcoin'),
    self: entry('consumer'),
    admin: entry('start-os'),
    stale: entry('old-dependency'),
  }
  assert.deepEqual(
    getLiveTasks(owner).map(([id]) => id),
    ['logical', 'self'],
  )
  assert.equal(owner.tasks.logical.task.packageId, 'bitcoin')
})

test('dependency status observes the selected provider and reacts to rebinding, without matching its physical version', async t => {
  const consumer = dependent()
  const alternate = pkg('alternate')
  alternate.statusInfo.health.rpc = { name: 'RPC', result: 'success' }
  const literal = pkg('bitcoin')
  literal.statusInfo.desired.main = 'stopped'
  literal.statusInfo.started = null
  const packages = { consumer, alternate, bitcoin: literal }
  const f = errors(t, packages)
  assert.equal((await f.read()).consumer.bitcoin, null)
  consumer.dependencyBindings.bitcoin.providerId = 'bitcoin'
  f.source.next(packages)
  assert.deepEqual((await f.read()).consumer.bitcoin, { type: 'notRunning' })
  consumer.dependencyBindings.bitcoin.providerId = null
  f.source.next(packages)
  assert.deepEqual((await f.read()).consumer.bitcoin, {
    type: 'selectionRequired',
  })
  delete consumer.dependencyBindings
  f.source.next(packages)
  assert.equal((await f.read()).consumer.bitcoin, null)
})

test('missing and incompatible installations have distinct dependency errors', async t => {
  const consumer = dependent('consumer', null)
  consumer.dependencyBindings.bitcoin.candidates = {}
  const packages = { consumer }
  const f = errors(t, packages)
  assert.deepEqual((await f.read()).consumer.bitcoin, { type: 'notInstalled' })
  for (const [state, type] of [
    ['installing', 'notInstalled'],
    ['installed', 'noCompatibleProvider'],
    ['updating', 'noCompatibleProvider'],
    ['removing', 'notInstalled'],
  ]) {
    packages.bitcoin = pkg('bitcoin')
    packages.bitcoin.stateInfo.state = state
    f.source.next(packages)
    assert.deepEqual((await f.read()).consumer.bitcoin, { type })
  }
})

test('provider updates retain the active dependency contract', async t => {
  const consumer = dependent()
  const alternate = pkg('alternate')
  alternate.statusInfo.health.rpc = { name: 'RPC', result: 'success' }
  alternate.stateInfo = {
    state: 'updating',
    manifest: alternate.stateInfo.manifest,
    installingInfo: { newManifest: { id: 'alternate', version: '99.0.0:0' } },
  }
  const f = errors(t, { consumer, alternate })
  assert.equal((await f.read()).consumer.bitcoin, null)
})

test('tasks remain associated with their logical requirement when providers overlap', async t => {
  const consumer = dependent()
  const alternate = pkg('alternate')
  alternate.statusInfo.health.rpc = { name: 'RPC', result: 'success' }
  consumer.currentDependencies.other = { kind: 'exists' }
  consumer.dependencyBindings.other = {
    providerId: 'alternate',
    versions: ['1.0.0:0'],
    candidates: {},
  }
  consumer.tasks.configure = {
    active: true,
    task: { packageId: 'other', severity: 'critical' },
  }
  const f = errors(t, { consumer, alternate })
  assert.equal((await f.read()).consumer.bitcoin, null)
  assert.deepEqual((await f.read()).consumer.other, { type: 'taskRequired' })
})

test('provider health and logical required tasks surface on the logical dependency', async t => {
  const consumer = dependent()
  const alternate = pkg('alternate')
  alternate.statusInfo.health.rpc = { name: 'RPC', result: 'failure' }
  const f = errors(t, { consumer, alternate })
  assert.equal((await f.read()).consumer.bitcoin.type, 'healthChecksFailed')
  consumer.tasks.configure = {
    active: true,
    task: { packageId: 'bitcoin', severity: 'critical' },
  }
  f.source.next({ consumer, alternate })
  assert.deepEqual((await f.read()).consumer.bitcoin, { type: 'taskRequired' })
})

test('transitive errors follow provider identities and cycles terminate', async t => {
  const consumer = dependent()
  const alternate = dependent('alternate', 'consumer')
  consumer.currentDependencies.bitcoin.kind = 'exists'
  alternate.currentDependencies.bitcoin.kind = 'exists'
  const f = errors(t, { consumer, alternate })
  assert.equal((await f.read()).consumer.bitcoin, null)
  alternate.currentDependencies.missing = { kind: 'exists' }
  alternate.dependencyBindings.missing = {
    providerId: null,
    versions: [],
    candidates: {},
  }
  f.source.next({ consumer, alternate })
  assert.deepEqual((await f.read()).consumer.bitcoin, { type: 'transitive' })
})

test('change warnings forward package-qualified metadata to the backend and render affected consumer names', async () => {
  const request = {
    id: 'alternate',
    version: '2.0.0',
    satisfies: [],
    satisfiesPackages: { bitcoin: ['26.0.0'] },
  }
  const calls = []
  const api = {
    rpc: {
      package: {
        previewProviderChange: async params => {
          calls.push(params)
          return ['consumer', 'unknown']
        },
      },
    },
  }
  assert.deepEqual(
    await dryUpdate(
      request,
      {
        consumer: pkg('consumer', {
          stateInfo: {
            state: 'installed',
            manifest: { id: 'consumer', title: 'Consumer title' },
          },
        }),
      },
      api,
    ),
    ['Consumer title', 'unknown'],
  )
  assert.deepEqual(calls, [request])
  api.rpc.package.previewProviderChange = async () => {
    throw new Error('offline')
  }
  await assert.rejects(dryUpdate(request, {}, api), /offline/)
})

test('logical task execution addresses the provider; dismissal addresses the task owner', async t => {
  const owner = dependent()
  const target = pkg('alternate', {
    actions: {
      configure: {
        name: 'Configure',
        allowedStatuses: 'any',
        visibility: 'enabled',
      },
    },
  })
  const presented = []
  const cleared = []
  const injector = createEnvironmentInjector([
    {
      provide: ActionService,
      useValue: { present: value => presented.push(value) },
    },
    { provide: DialogService, useValue: { openConfirm: () => of(true) } },
    {
      provide: ApiService,
      useValue: {
        rpc: {
          package: {
            action: { clearTask: async value => cleared.push(value) },
          },
        },
      },
    },
    { provide: TaskService, useValue: { run: async work => work() } },
    { provide: ServiceTasksComponent, useValue: { pkg: () => owner } },
    { provide: i18nPipe, useValue: { transform: value => value } },
    { provide: PackageIconService, useValue: { get: () => of('icon') } },
  ])
  t.after(() => injector.destroy())
  const component = runInInjectionContext(
    injector,
    () => new ServiceTaskComponent(),
  )
  component.task = signal({
    packageId: 'bitcoin',
    actionId: 'configure',
    replayId: 'logical-configure',
    severity: 'important',
  })
  component.services = signal({ alternate: target })
  assert.equal(component.disabled(), false)
  await component.handle()
  assert.equal(presented[0].pkgInfo.id, 'alternate')
  await component.dismiss()
  assert.deepEqual(cleared, [
    { packageId: 'consumer', replayId: 'logical-configure', force: false },
  ])
})

const { ServiceDependenciesComponent } =
  await import('../src/app/routes/portal/routes/services/components/dependencies.component.ts')

test('provider selector persists owner-scoped preferences, clears Automatic, and rolls back a rejected choice', async t => {
  const owner = dependent()
  owner.dependencySelections.bitcoin = 'alternate'
  const calls = []
  const resets = []
  let reject = false
  const injector = createEnvironmentInjector([
    {
      provide: ApiService,
      useValue: {
        rpc: {
          package: {
            selectDependencyProvider: async request => {
              calls.push(request)
            },
          },
        },
      },
    },
    {
      provide: TaskService,
      useValue: {
        runInline: async work => {
          if (reject) return false
          await work()
          return true
        },
      },
    },
    { provide: i18nPipe, useValue: { transform: value => value } },
  ])
  t.after(() => injector.destroy())
  const component = runInInjectionContext(
    injector,
    () => new ServiceDependenciesComponent(),
  )
  component.pkg = signal(owner)
  const alternate = pkg('alternate')
  alternate.stateInfo.manifest.title = 'Alternate provider'
  component.services = signal({ alternate })
  assert.equal(
    component.stringify('alternate'),
    'Alternate provider (alternate)',
  )
  assert.equal(component.stringify('unavailable'), 'unavailable')
  assert.equal(component.stringify(''), 'Automatic')
  const control = { reset: value => resets.push(value) }
  await component.select('bitcoin', 'alternate', control)
  await component.select('bitcoin', '', control)
  assert.deepEqual(calls, [
    { id: 'consumer', dependencyId: 'bitcoin', providerId: 'alternate' },
    { id: 'consumer', dependencyId: 'bitcoin', providerId: null },
  ])
  reject = true
  await component.select('bitcoin', 'bitcoin', control)
  assert.deepEqual(resets, ['alternate'])
  assert.equal(component.saving(), false)
})

const { StandardActionsService } =
  await import('../src/app/services/standard-actions.service.ts')

test('removal warnings query backend alias impacts and preserve the physical uninstall target', async () => {
  const previews = []
  const warnings = []
  const uninstalls = []
  let affected = ['consumer']
  const instance = {
    tasks: {
      runInline: async work => {
        await work()
        return true
      },
    },
    i18n: { transform: value => value },
    api: {
      rpc: {
        package: {
          previewProviderChange: async params => {
            previews.push(params)
            return affected
          },
        },
      },
    },
    dialog: {
      openConfirm: options => {
        warnings.push(options)
        return of(false)
      },
    },
    doUninstall: async params => uninstalls.push(params),
  }
  const manifest = { id: 'alternate', title: 'Alternate' }
  await StandardActionsService.prototype.uninstall.call(instance, manifest, {
    force: false,
    soft: true,
  })
  assert.deepEqual(previews[0], {
    id: 'alternate',
    version: null,
    satisfies: [],
    satisfiesPackages: {},
  })
  assert.equal(warnings.length, 1)
  assert.match(warnings[0].data.content, /Services that depend on Alternate/)
  assert.deepEqual(uninstalls, [])
  affected = []
  await StandardActionsService.prototype.uninstall.call(instance, manifest, {
    force: false,
    soft: true,
  })
  assert.deepEqual(uninstalls, [{ id: 'alternate', force: false, soft: true }])
})
