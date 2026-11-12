import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject } from 'rxjs'
import { PatchDB } from 'patch-db-client'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export class DialogService {} export class TaskService {} export class i18nPipe {} export class LocalizePipe {}',
    ],
    [
      '@taiga-ui/core',
      'export class TuiButton {} export class TuiCell {} export class TuiDataList {} export class TuiDropdown {} export class TuiTitle {} export const tuiButtonOptionsProvider=()=>[]; export const tuiIconStart=()=>{}',
    ],
    [
      '@taiga-ui/kit',
      'export class TuiBadge {} export class TuiButtonLoading {} export class TuiSwitch {}',
    ],
    ['@taiga-ui/layout', 'export class TuiForm {} export class TuiHeader {}'],
    [
      '@taiga-ui/polymorpheus',
      'let context; export const setContext=value=>context=value; export const injectContext=()=>context; export class PolymorpheusComponent { constructor(component) { this.component=component } }',
    ],
    [
      '@taiga-ui/cdk',
      'export const tuiDefaultSort=(a,b)=>a.id.localeCompare(b.id)',
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    ['src/app/services/config.service', 'export class ConfigService {}'],
    [
      'src/app/services/pkg-status-rendering.service',
      'export const PrimaryRendering={running:{display:"Running"},stopped:{display:"Stopped"}}; export const renderPkgStatus=pkg=>({primary:pkg.status??"running"})',
    ],
    ['src/app/utils/acme', 'export const toAuthorityName=()=>""'],
  ]),
)
const { TaskService } = await import('@start9labs/shared')
const { setContext } = await import('@taiga-ui/polymorpheus')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { ConfigService } = await import('src/app/services/config.service')
const { HTTP_PROXY_CHAIN, HTTP_PROXY_PLUGINS } =
  await import('../src/app/routes/portal/components/interfaces/proxy-chain.ts')
const { LocalPackagesService } =
  await import('../src/app/services/local-packages.service.ts')
const pluginProviders = [
  LocalPackagesService,
  { provide: HTTP_PROXY_PLUGINS, useFactory: HTTP_PROXY_PLUGINS.ɵprov.factory },
]
const { InterfaceService } =
  await import('../src/app/routes/portal/components/interfaces/interface.service.ts')

const network = gatewayId => ({ kind: 'network', gatewayId })
const stage = profileId => ({ packageId: 'waf', profileId })
const policy = (gateway, chain = [], publicOnly = false) => ({
  gateway,
  chain,
  publicOnly,
})
const address = {
  internalPort: 8080,
  hostId: 'web',
  scheme: 'http',
  sslScheme: 'https',
  username: null,
  suffix: '',
}
const binding = proxyChains => ({
  options: { proxy: { type: 'http' }, addSsl: null, secure: null },
  proxyChains,
  addresses: { available: [], enabled: [], disabled: [] },
})

function dialog(t, gateway = network('eth0')) {
  const packages = new BehaviorSubject({
    app: {
      hosts: {
        web: {
          bindings: {
            8080: binding([
              policy(network('eth0'), [stage('edge')]),
              policy(network('eth1'), [stage('app')]),
              policy({ kind: 'url-plugin', packageId: 'tor' }, [stage('edge')]),
            ]),
          },
        },
      },
      stateInfo: { state: 'installed', manifest: { plugins: [] } },
    },
  })
  const submissions = [],
    closed = []
  const submit = async params => {
    submissions.push(params)
    return null
  }
  setContext({
    id: 'proxy',
    data: { packageId: 'app', address, gateway, gatewayName: 'Gateway' },
    completeWith: value => closed.push(value),
  })
  const injector = createEnvironmentInjector([
    pluginProviders,
    { provide: PatchDB, useValue: { watch$: () => packages } },
    {
      provide: ApiService,
      useValue: {
        rpc: { package: { host: { binding: { setHttpProxyChain: submit } } } },
      },
    },
    {
      provide: TaskService,
      useValue: {
        runInline: async action => {
          await action()
          return true
        },
      },
    },
  ])
  t.after(() => injector.destroy())
  const instance = runInInjectionContext(
    injector,
    () => new HTTP_PROXY_CHAIN.component(),
  )
  return { instance, packages, submissions, closed }
}

test('a gateway draft survives sibling policy and provider status updates', t => {
  const { instance, packages } = dialog(t)
  instance.add(stage('app'))
  instance.setPublicOnly(true)
  const draft = structuredClone(instance.draft())
  const sibling = structuredClone(packages.value)
  sibling.app.hosts.web.bindings[8080].proxyChains[1].chain = []
  sibling.waf = {
    stateInfo: {
      state: 'installed',
      manifest: { id: 'waf', title: 'WAF', plugins: ['http-proxy-v0'] },
    },
    plugin: { httpProxy: { profiles: [] } },
    status: 'stopped',
  }
  packages.next(sibling)
  assert.deepEqual(instance.draft(), draft)
  assert.equal(instance.dirty(), true)
  assert.equal(instance.profiles().size, 0)
  assert.equal(instance.draft().chain.length, 2)
  const changed = structuredClone(sibling)
  changed.app.hosts.web.bindings[8080].proxyChains[0] = policy(
    network('eth0'),
    [stage('external')],
  )
  packages.next(changed)
  assert.deepEqual(instance.draft().chain, [stage('external')])
  assert.equal(instance.dirty(), false)
})

test('saving submits only the selected gateway and its public-only intent', async t => {
  const { instance, submissions, closed } = dialog(t)
  instance.setPublicOnly(true)
  await instance.save()
  assert.deepEqual(submissions, [
    {
      package: 'app',
      host: 'web',
      internalPort: 8080,
      policy: policy(network('eth0'), [stage('edge')], true),
    },
  ])
  assert.equal(closed.length, 1)
})

test('URL gateway edits submit false public-only without changing source identity', async t => {
  const gateway = { kind: 'url-plugin', packageId: 'tor' }
  const { instance, submissions } = dialog(t, gateway)
  instance.add(stage('app'))
  await instance.save()
  assert.deepEqual(
    submissions[0].policy,
    policy(gateway, [stage('edge'), stage('app')]),
  )
})

test('OS binding edits use the server host and server setter', async t => {
  const submissions = []
  const host = new BehaviorSubject({ bindings: { 80: binding([]) } })
  setContext({
    id: 'os',
    data: {
      packageId: 'start-os',
      address: { ...address, hostId: 'admin', internalPort: 80 },
      gateway: network('eth0'),
      gatewayName: 'Home',
    },
    completeWith() {},
  })
  const injector = createEnvironmentInjector([
    pluginProviders,
    {
      provide: PatchDB,
      useValue: {
        watch$: key => (key === 'serverInfo' ? host : new BehaviorSubject({})),
      },
    },
    {
      provide: ApiService,
      useValue: {
        rpc: {
          server: {
            host: {
              binding: {
                setHttpProxyChain: async params => submissions.push(params),
              },
            },
          },
        },
      },
    },
    {
      provide: TaskService,
      useValue: {
        runInline: async action => {
          await action()
          return true
        },
      },
    },
  ])
  t.after(() => injector.destroy())
  const instance = runInInjectionContext(
    injector,
    () => new HTTP_PROXY_CHAIN.component(),
  )
  instance.setPublicOnly(true)
  await instance.save()
  assert.deepEqual(submissions, [
    { internalPort: 80, policy: policy(network('eth0'), [], true) },
  ])
})

test('installed proxy plugins remain eligible while stopped, unpublished or updating', t => {
  const packages = new BehaviorSubject({})
  const injector = createEnvironmentInjector([
    pluginProviders,
    { provide: PatchDB, useValue: { watch$: () => packages } },
  ])
  t.after(() => injector.destroy())
  const plugins = injector.get(HTTP_PROXY_PLUGINS)
  const manifest = { plugins: ['http-proxy-v0'] }
  assert.deepEqual(plugins(), [])
  packages.next({
    waf: {
      stateInfo: {
        state: 'installing',
        installingInfo: { newManifest: manifest },
      },
    },
  })
  assert.deepEqual(plugins(), [])
  packages.next({
    waf: {
      stateInfo: { state: 'installed', manifest },
      status: 'stopped',
      plugin: { httpProxy: null },
    },
  })
  assert.deepEqual(
    plugins().map(([id]) => id),
    ['waf'],
  )
  packages.next({
    waf: {
      stateInfo: {
        state: 'updating',
        manifest,
        installingInfo: { newManifest: { plugins: [] } },
      },
    },
  })
  assert.deepEqual(
    plugins().map(([id]) => id),
    ['waf'],
  )
  packages.next({ waf: { stateInfo: { state: 'removing', manifest } } })
  assert.deepEqual(plugins(), [])
  packages.next({
    other: { stateInfo: { state: 'installed', manifest: { plugins: [] } } },
  })
  assert.deepEqual(plugins(), [])
})

test('saved network and URL policies remain discoverable without addresses or providers', t => {
  const injector = createEnvironmentInjector([
    { provide: ConfigService, useValue: { accessType: 'localhost' } },
  ])
  t.after(() => injector.destroy())
  const service = runInInjectionContext(injector, () => new InterfaceService())
  const host = {
    bindings: {
      8080: binding([
        policy(network('eth0')),
        policy(network('retired')),
        policy({ kind: 'url-plugin', packageId: 'retired' }),
      ]),
    },
  }
  const iface = { addressInfo: address, type: 'ui', masked: false }
  const networks = service.getGatewayGroups(iface, host, [
    { id: 'eth0', name: 'Home', ipInfo: { deviceType: 'ethernet' } },
  ])
  assert.deepEqual(
    networks.map(group => [
      group.gatewayId,
      group.gatewayName,
      group.addresses,
    ]),
    [
      ['eth0', 'Home', []],
      ['retired', 'retired', []],
    ],
  )
  const plugins = service.getPluginGroups(iface, host)
  assert.deepEqual(
    plugins.map(group => [group.pluginId, group.addresses]),
    [['retired', []]],
  )
})
