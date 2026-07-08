import {
  ExtendedVersion,
  FileHelper,
  getDataVersion,
  overlaps,
  types as T,
  utils,
  VersionRange,
} from '@start9labs/start-sdk'
import * as fs from 'fs/promises'

import { polyfillEffects } from './polyfillEffects'
import { fromDuration } from '../../../Models/Duration'
import { System } from '../../../Interfaces/System'
import { matchManifest, Manifest } from './matchManifest'
import * as childProcess from 'node:child_process'
import { DockerProcedureContainer } from './DockerProcedureContainer'
import { DockerProcedure } from '../../../Models/DockerProcedure'
import { promisify } from 'node:util'
import * as U from './oldEmbassyTypes'
import { MainLoop } from './MainLoop'
import { z } from '@start9labs/start-sdk'
import { AddSslOptions } from '@start9labs/start-core/osBindings'
import {
  BindOptionsByProtocol,
  MultiHost,
} from '@start9labs/start-core/interfaces/Host'
import { ServiceInterfaceBuilder } from '@start9labs/start-core/interfaces/ServiceInterfaceBuilder'
import { Effects } from '../../../Models/Effects'
import {
  OldConfigSpec,
  matchOldConfigSpec,
  transformConfigSpec,
  transformNewConfigToOld,
  transformOldConfigToNew,
} from './transformConfigSpec'
import { partialDiff } from '@start9labs/start-core/util'
import { Volume } from '@start9labs/start-sdk/lib/util/Volume'

type Optional<A> = A | undefined | null
function todo(): never {
  throw new Error('Not implemented')
}

function getStatus(
  effects: Effects,
  options: Omit<Parameters<Effects['getStatus']>[0], 'callback'> = {},
) {
  async function* watch(abort?: AbortSignal) {
    const resolveCell = { resolve: () => {} }
    effects.onLeaveContext(() => {
      resolveCell.resolve()
    })
    abort?.addEventListener('abort', () => resolveCell.resolve())
    while (effects.isInContext && !abort?.aborted) {
      let callback: () => void = () => {}
      const waitForNext = new Promise<void>(resolve => {
        callback = resolve
        resolveCell.resolve = resolve
      })
      yield await effects.getStatus({ ...options, callback })
      await waitForNext
    }
  }
  return {
    const: () =>
      effects.getStatus({
        ...options,
        callback:
          effects.constRetry &&
          (() => effects.constRetry && effects.constRetry()),
      }),
    once: () => effects.getStatus(options),
    watch: (abort?: AbortSignal) => {
      const ctrl = new AbortController()
      abort?.addEventListener('abort', () => ctrl.abort())
      return watch(ctrl.signal)
    },
    onChange: (
      callback: (
        value: T.StatusInfo | null,
        error?: Error,
      ) => { cancel: boolean } | Promise<{ cancel: boolean }>,
    ) => {
      ;(async () => {
        const ctrl = new AbortController()
        for await (const value of watch(ctrl.signal)) {
          try {
            const res = await callback(value)
            if (res.cancel) {
              ctrl.abort()
              break
            }
          } catch (e) {
            console.error(
              'callback function threw an error @ getStatus.onChange',
              e,
            )
          }
        }
      })()
        .catch(e => callback(null, e as Error))
        .catch(e =>
          console.error(
            'callback function threw an error @ getStatus.onChange',
            e,
          ),
        )
    },
  }
}

/**
 * Local type for procedure values from the manifest.
 * The manifest's zod schemas use ZodTypeAny casts that produce `unknown` in zod v4.
 * This type restores the expected shape for type-safe property access.
 */
type Procedure =
  | (DockerProcedure & { type: 'docker' })
  | { type: 'script'; args: unknown[] | null }

const MANIFEST_LOCATION = '/usr/lib/startos/package/embassyManifest.json'
export const EMBASSY_JS_LOCATION = '/usr/lib/startos/package/embassy.js'

const configFile = FileHelper.json(
  {
    base: new Volume('embassy'),
    subpath: 'config.json',
  },
  z.any(),
)
const dependsOnFile = FileHelper.json(
  {
    base: new Volume('embassy'),
    subpath: 'dependsOn.json',
  },
  z.record(z.string(), z.array(z.string())),
)

const matchResult = z.object({
  result: z.any(),
})
const matchError = z.object({
  error: z.string(),
})
const matchErrorCode = z.object({
  'error-code': z.tuple([z.number(), z.string()]),
})

const assertNever = (
  x: never,
  message = 'Not expecting to get here: ',
): never => {
  throw new Error(message + JSON.stringify(x))
}
/**
  Should be changing the type for specific properties, and this is mostly a transformation for the old return types to the newer one.
*/
function isMatchResult(a: unknown): a is z.infer<typeof matchResult> {
  return matchResult.safeParse(a).success
}
function isMatchError(a: unknown): a is z.infer<typeof matchError> {
  return matchError.safeParse(a).success
}
function isMatchErrorCode(a: unknown): a is z.infer<typeof matchErrorCode> {
  return matchErrorCode.safeParse(a).success
}
const fromReturnType = <A>(a: U.ResultType<A>): A => {
  if (isMatchResult(a)) {
    return a.result
  }
  if (isMatchError(a)) {
    console.info({ passedErrorStack: new Error().stack, error: a.error })
    throw { error: a.error }
  }
  if (isMatchErrorCode(a)) {
    const [code, message] = a['error-code']
    throw { error: message, code }
  }
  return assertNever(a as never)
}

const matchSetResult = z.object({
  'depends-on': z.record(z.string(), z.array(z.string())).nullable().optional(),
  dependsOn: z.record(z.string(), z.array(z.string())).nullable().optional(),
  signal: z.enum([
    'SIGTERM',
    'SIGHUP',
    'SIGINT',
    'SIGQUIT',
    'SIGILL',
    'SIGTRAP',
    'SIGABRT',
    'SIGBUS',
    'SIGFPE',
    'SIGKILL',
    'SIGUSR1',
    'SIGSEGV',
    'SIGUSR2',
    'SIGPIPE',
    'SIGALRM',
    'SIGSTKFLT',
    'SIGCHLD',
    'SIGCONT',
    'SIGSTOP',
    'SIGTSTP',
    'SIGTTIN',
    'SIGTTOU',
    'SIGURG',
    'SIGXCPU',
    'SIGXFSZ',
    'SIGVTALRM',
    'SIGPROF',
    'SIGWINCH',
    'SIGIO',
    'SIGPWR',
    'SIGSYS',
    'SIGINFO',
  ]),
})

type OldGetConfigRes = {
  config?: null | Record<string, unknown>
  spec: OldConfigSpec
}

export type PropertiesValue =
  | {
      /** The type of this value, either "string" or "object" */
      type: 'object'
      /** A nested mapping of values. The user will experience this as a nested page with back button */
      value: { [k: string]: PropertiesValue }
      /** (optional) A human readable description of the new set of values */
      description: string | null
    }
  | {
      /** The type of this value, either "string" or "object" */
      type: 'string'
      /** The value to display to the user */
      value: string
      /** A human readable description of the value */
      description: string | null
      /** Whether or not to mask the value, for example, when displaying a password */
      masked: boolean | null
      /** Whether or not to include a button for copying the value to clipboard */
      copyable: boolean | null
      /** Whether or not to include a button for displaying the value as a QR code */
      qr: boolean | null
    }

export type PropertiesReturn = {
  [key: string]: PropertiesValue
}

export type PackagePropertiesV2 = {
  [name: string]: PackagePropertyObject | PackagePropertyString
}
export type PackagePropertyString = {
  type: 'string'
  description?: string | null
  value: string
  /** Let's the ui make this copyable button */
  copyable?: boolean | null
  /** Let the ui create a qr for this field */
  qr?: boolean | null
  /** Hiding the value unless toggled off for field */
  masked?: boolean | null
}
export type PackagePropertyObject = {
  value: PackagePropertiesV2
  type: 'object'
  description: string
}

const asProperty_ = (
  x: PackagePropertyString | PackagePropertyObject,
): PropertiesValue => {
  if (x.type === 'object') {
    return {
      ...x,
      value: Object.fromEntries(
        Object.entries(x.value).map(([key, value]) => [
          key,
          asProperty_(value),
        ]),
      ),
    }
  }
  return {
    masked: false,
    description: null,
    qr: null,
    copyable: null,
    ...x,
  }
}
const asProperty = (x: PackagePropertiesV2): PropertiesReturn =>
  Object.fromEntries(
    Object.entries(x).map(([key, value]) => [key, asProperty_(value)]),
  )
const matchPackagePropertyObject: z.ZodType<PackagePropertyObject> = z.object({
  value: z.lazy(() => matchPackageProperties),
  type: z.literal('object'),
  description: z.string(),
})

const matchPackagePropertyString: z.ZodType<PackagePropertyString> = z.object({
  type: z.literal('string'),
  description: z.string().nullable().optional(),
  value: z.string(),
  copyable: z.boolean().nullable().optional(),
  qr: z.boolean().nullable().optional(),
  masked: z.boolean().nullable().optional(),
})
const matchPackageProperties: z.ZodType<PackagePropertiesV2> = z.lazy(() =>
  z.record(
    z.string(),
    z.union([matchPackagePropertyObject, matchPackagePropertyString]),
  ),
)

const matchProperties = z.object({
  version: z.literal(2),
  data: matchPackageProperties,
})

function convertProperties(
  name: string,
  value: PropertiesValue,
): T.ActionResultMember {
  if (value.type === 'string') {
    return {
      type: 'single',
      name,
      description: value.description,
      copyable: value.copyable || false,
      masked: value.masked || false,
      qr: value.qr || false,
      value: value.value,
    }
  }
  return {
    type: 'group',
    name,
    description: value.description,
    value: Object.entries(value.value).map(([name, value]) =>
      convertProperties(name, value),
    ),
  }
}

export class SystemForEmbassy implements System {
  private version: ExtendedVersion
  currentRunning: MainLoop | undefined
  static async of(manifestLocation: string = MANIFEST_LOCATION) {
    const moduleCode = await import(EMBASSY_JS_LOCATION)
      .catch(_ => require(EMBASSY_JS_LOCATION))
      .catch(async _ => {
        console.error(utils.asError('Could not load the js'))
        console.error({
          exists: await fs.stat(EMBASSY_JS_LOCATION),
        })
        return {}
      })
    const manifestData = await fs.readFile(manifestLocation, 'utf-8')
    return new SystemForEmbassy(
      matchManifest.parse(JSON.parse(manifestData)),
      moduleCode,
    )
  }

  constructor(
    readonly manifest: Manifest,
    readonly moduleCode: Partial<U.ExpectedExports>,
  ) {
    this.version = ExtendedVersion.parseEmver(manifest.version)
    if (
      this.manifest.id === 'bitcoind' &&
      this.manifest.title.toLowerCase().includes('knots')
    )
      this.version.flavor = 'knots'

    if (
      this.manifest.id === 'lnd' ||
      this.manifest.id === 'ride-the-lightning' ||
      this.manifest.id === 'datum'
    ) {
      this.version.upstream.prerelease = ['beta']
    } else if (
      this.manifest.id === 'lightning-terminal' ||
      this.manifest.id === 'robosats'
    ) {
      this.version.upstream.prerelease = ['alpha']
    }

    if (this.manifest.id === 'nostr') {
      this.manifest.id = 'nostr-rs-relay'
    }
    if (this.manifest.id === 'ghost') {
      this.manifest.id = 'ghost-legacy'
    }
    if (this.manifest.id === 'synapse') {
      this.manifest.id = 'synapse-legacy'
    }
    if (this.manifest.id === 'monerod') {
      this.manifest.id = 'monerod-legacy'
    }
    if (this.manifest.id === 'fedimintd') {
      this.manifest.id = 'fedimint-guardian'
    }
  }

  async init(
    effects: Effects,
    kind: 'install' | 'update' | 'restore' | null,
  ): Promise<void> {
    if (kind === 'restore') {
      await this.restoreBackup(effects, null)
    }
    for (let depId in this.manifest.dependencies) {
      if (this.manifest.dependencies[depId]?.config) {
        await this.dependenciesAutoconfig(effects, depId, null)
      }
    }
    await effects.setMainStatus({ status: 'stopped' })
    await this.exportActions(effects)
    await this.exportNetwork(effects)
    await this.containerSetDependencies(effects)
    if (kind === 'install' || kind === 'update') {
      await this.packageInit(effects, null)
    }
  }
  async containerSetDependencies(effects: T.Effects) {
    const oldDeps: Record<string, string[]> = Object.fromEntries(
      await effects
        .getDependencies()
        .then(x =>
          x.flatMap(x =>
            x.kind === 'running' ? [[x.id, x?.healthChecks || []]] : [],
          ),
        )
        .catch(() => []),
    )
    await this.setDependencies(effects, oldDeps, false)
  }

  async exit(): Promise<void> {
    if (this.currentRunning) await this.currentRunning.clean()
    delete this.currentRunning
  }

  async start(effects: T.Effects): Promise<void> {
    effects.constRetry = utils.once(() => effects.restart())
    if (!!this.currentRunning) return

    this.currentRunning = await MainLoop.of(this, effects)
  }
  callCallback(_callback: number, _args: any[]): void {}
  async stop(): Promise<void> {
    const clean = this.currentRunning?.clean({
      timeout: fromDuration(
        (this.manifest.main['sigterm-timeout'] as any) || '30s',
      ),
    })
    delete this.currentRunning
    if (clean) {
      await clean
    }
  }

  async packageInit(effects: Effects, timeoutMs: number | null): Promise<void> {
    const previousVersion = await getDataVersion(effects)
    if (previousVersion) {
      const migrationRes = await this.migration(
        effects,
        { from: previousVersion },
        timeoutMs,
      )
      if (migrationRes) {
        if (migrationRes.configured)
          await effects.action.clearTasks({ only: ['needs-config'] })
        await configFile.write(
          effects,
          await this.getConfig(effects, timeoutMs),
        )
      }
    } else if (this.manifest.config) {
      await effects.action.createTask({
        packageId: this.manifest.id,
        actionId: 'config',
        severity: 'critical',
        replayId: 'needs-config',
        reason: 'This service must be configured before it can be run',
      })
    }

    await effects.setDataVersion({
      version: this.version.toString(),
    })
    // @FullMetal: package hacks go here
  }
  async exportNetwork(effects: Effects) {
    for (const [id, interfaceValue] of Object.entries(
      this.manifest.interfaces,
    )) {
      const host = new MultiHost({ effects, id })
      const internalPorts = new Set(
        Object.values(interfaceValue['tor-config']?.['port-mapping'] ?? {})
          .map(v => parseInt(v))
          .concat(
            ...Object.values(interfaceValue['lan-config'] ?? {}).map(
              c => c.internal,
            ),
          )
          .filter(Boolean),
      )
      const bindings = Array.from(internalPorts).map<
        [number, BindOptionsByProtocol]
      >(port => {
        const lanPort = Object.entries(interfaceValue['lan-config'] ?? {}).find(
          ([external, internal]) => internal.internal === port,
        )?.[0]
        const torPort = Object.entries(
          interfaceValue['tor-config']?.['port-mapping'] ?? {},
        ).find(
          ([external, internal]) => Number.parseInt(internal) === port,
        )?.[0]
        let addSsl: AddSslOptions | null = null
        if (lanPort) {
          const lanPortNum = Number.parseInt(lanPort)
          if (lanPortNum === 443) {
            return [port, { protocol: 'http', preferredExternalPort: 80 }]
          }
          addSsl = {
            preferredExternalPort: lanPortNum,
            alpn: { specified: [] },
            addXForwardedHeaders: false,
            auth: null,
          }
        }
        return [
          port,
          {
            protocol: null,
            secure: null,
            preferredExternalPort: Number.parseInt(
              torPort || lanPort || String(port),
            ),
            addSsl,
          },
        ]
      })

      await Promise.all(
        bindings.map(async ([internal, options]) => {
          if (internal == null) {
            return
          }
          if (options?.preferredExternalPort == null) {
            return
          }
          const origin = await host.bindPort(internal, options)
          await origin.export([
            new ServiceInterfaceBuilder({
              effects,
              name: interfaceValue.name,
              id: `${id}-${internal}`,
              description: interfaceValue.description,
              type:
                interfaceValue.ui &&
                (origin.scheme === 'http' || origin.sslScheme === 'https')
                  ? 'ui'
                  : 'api',
              masked: false,
              path: '',
              schemeOverride: null,
              query: {},
              username: null,
            }),
          ])
        }),
      )
    }
  }
  async getActionInput(
    effects: Effects,
    actionId: string,
    _prefill: Record<string, unknown> | null,
    timeoutMs: number | null,
  ): Promise<T.ActionInput | null> {
    if (actionId === 'config') {
      const config = await this.getConfig(effects, timeoutMs)
      return {
        eventId: effects.eventId!,
        spec: config.spec,
        value: config.config,
      }
    } else if (actionId === 'properties') {
      return null
    } else {
      const oldSpec = this.manifest.actions?.[actionId]?.['input-spec']
      if (!oldSpec) return null
      return {
        eventId: effects.eventId!,
        spec: transformConfigSpec(oldSpec as OldConfigSpec),
        value: null,
      }
    }
  }
  async runAction(
    effects: Effects,
    actionId: string,
    input: unknown,
    timeoutMs: number | null,
  ): Promise<T.ActionResult | null> {
    if (actionId === 'config') {
      await this.setConfig(effects, input, timeoutMs)
      return null
    } else if (actionId === 'properties') {
      return {
        version: '1',
        title: 'Properties',
        message: null,
        result: {
          type: 'group',
          value: Object.entries(await this.properties(effects, timeoutMs)).map(
            ([name, value]) => convertProperties(name, value),
          ),
        },
      }
    } else {
      return this.action(effects, actionId, input, timeoutMs)
    }
  }
  async exportActions(effects: Effects) {
    const manifest = this.manifest
    const actions = {
      ...manifest.actions,
    }
    if (manifest.config) {
      actions.config = {
        name: 'Configure',
        description: `Customize ${manifest.title}`,
        'allowed-statuses': ['running', 'stopped'],
        'input-spec': {},
        implementation: { type: 'script', args: [] },
      }
    }
    if (manifest.properties) {
      actions.properties = {
        name: 'Properties',
        description:
          'Runtime information, credentials, and other values of interest',
        'allowed-statuses': ['running', 'stopped'],
        'input-spec': null,
        implementation: { type: 'script', args: [] },
      }
    }
    for (const [actionId, action] of Object.entries(actions)) {
      const hasRunning = !!action['allowed-statuses'].find(x => x === 'running')
      const hasStopped = !!action['allowed-statuses'].find(x => x === 'stopped')
      // prettier-ignore
      const allowedStatuses = hasRunning && hasStopped ? "any":
        hasRunning ? "only-running" :
         "only-stopped"
      await effects.action.export({
        id: actionId,
        metadata: {
          name: action.name,
          description: action.description,
          warning: action.warning || null,
          visibility: 'enabled',
          allowedStatuses,
          hasInput: !!action['input-spec'],
          group: null,
        },
      })
    }
    await effects.action.clear({ except: Object.keys(actions) })
  }
  async uninit(
    effects: Effects,
    target: ExtendedVersion | VersionRange | null,
    timeoutMs?: number | null,
  ): Promise<void> {
    await this.currentRunning?.clean({ timeout: timeoutMs ?? undefined })
    if (target) {
      await this.migration(effects, { to: target }, timeoutMs ?? null)
    }
    await effects.setMainStatus({ status: 'stopped' })
  }

  async createBackup(
    effects: Effects,
    timeoutMs: number | null,
  ): Promise<void> {
    const backup = this.manifest.backup.create as Procedure
    if (backup.type === 'docker') {
      const commands = [backup.entrypoint, ...backup.args]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        backup,
        {
          ...this.manifest.volumes,
          BACKUP: { type: 'backup', readonly: false },
        },
        `Backup - ${commands.join(' ')}`,
      )
      await container.execFail(commands, timeoutMs)
    } else {
      const moduleCode = await this.moduleCode
      await moduleCode.createBackup?.(polyfillEffects(effects, this.manifest))
    }
    const dataVersion = await effects.getDataVersion()
    if (dataVersion)
      await fs.writeFile('/media/startos/backup/dataVersion.txt', dataVersion, {
        encoding: 'utf-8',
      })
  }
  async restoreBackup(
    effects: Effects,
    timeoutMs: number | null,
  ): Promise<void> {
    const store = await fs
      .readFile('/media/startos/backup/store.json', {
        encoding: 'utf-8',
      })
      .catch(_ => null)
    const restoreBackup = this.manifest.backup.restore as Procedure
    if (restoreBackup.type === 'docker') {
      const commands = [restoreBackup.entrypoint, ...restoreBackup.args]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        restoreBackup,
        {
          ...this.manifest.volumes,
          BACKUP: { type: 'backup', readonly: true },
        },
        `Restore Backup - ${commands.join(' ')}`,
      )
      await container.execFail(commands, timeoutMs)
    } else {
      const moduleCode = await this.moduleCode
      await moduleCode.restoreBackup?.(polyfillEffects(effects, this.manifest))
    }

    const dataVersion = await fs
      .readFile('/media/startos/backup/dataVersion.txt', {
        encoding: 'utf-8',
      })
      .catch(_ => null)
    if (dataVersion) await effects.setDataVersion({ version: dataVersion })
  }
  async getConfig(effects: Effects, timeoutMs: number | null) {
    return this.getConfigUncleaned(effects, timeoutMs).then(convertToNewConfig)
  }
  private async getConfigUncleaned(
    effects: Effects,
    timeoutMs: number | null,
  ): Promise<OldGetConfigRes> {
    const config = this.manifest.config?.get as Procedure | undefined
    if (!config) return { spec: {} }
    if (config.type === 'docker') {
      const commands = [config.entrypoint, ...config.args]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        config,
        this.manifest.volumes,
        `Get Config - ${commands.join(' ')}`,
      )
      // TODO: yaml
      return JSON.parse(
        (await container.execFail(commands, timeoutMs)).stdout.toString(),
      )
    } else {
      const moduleCode = await this.moduleCode
      const method = moduleCode.getConfig
      if (!method) throw new Error('Expecting that the method getConfig exists')
      return (await method(polyfillEffects(effects, this.manifest)).then(x => {
        if ('result' in x) return JSON.parse(JSON.stringify(x.result))
        if ('error' in x) throw new Error('Error getting config: ' + x.error)
        throw new Error('Error getting config: ' + x['error-code'][1])
      })) as any
    }
  }
  async setConfig(
    effects: Effects,
    newConfigWithoutPointers: unknown,
    timeoutMs: number | null,
  ): Promise<void> {
    const spec = await this.getConfigUncleaned(effects, timeoutMs).then(
      x => x.spec,
    )
    const newConfig = transformNewConfigToOld(
      spec,
      structuredClone(newConfigWithoutPointers as Record<string, unknown>),
    )
    await updateConfig(effects, this.manifest, spec, newConfig)
    await configFile.write(effects, newConfig)
    const setConfigValue = this.manifest.config?.set as Procedure | undefined
    if (!setConfigValue) return
    if (setConfigValue.type === 'docker') {
      const commands = [
        setConfigValue.entrypoint,
        ...setConfigValue.args,
        JSON.stringify(newConfig),
      ]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        setConfigValue,
        this.manifest.volumes,
        `Set Config - ${commands.join(' ')}`,
      )
      const answer = matchSetResult.parse(
        JSON.parse(
          (await container.execFail(commands, timeoutMs)).stdout.toString(),
        ),
      )
      const dependsOn = answer['depends-on'] ?? answer.dependsOn ?? {}
      await this.setDependencies(effects, dependsOn, true)
      return
    } else if (setConfigValue.type === 'script') {
      const moduleCode = await this.moduleCode
      const method = moduleCode.setConfig
      if (!method) throw new Error('Expecting that the method setConfig exists')

      const answer = matchSetResult.parse(
        await method(
          polyfillEffects(effects, this.manifest),
          newConfig as U.Config,
        ).then((x): T.SetResult => {
          if ('result' in x)
            return {
              dependsOn: x.result['depends-on'],
              signal:
                x.result.signal === 'SIGEMT' ? 'SIGTERM' : x.result.signal,
            }
          if ('error' in x) throw new Error('Error getting config: ' + x.error)
          throw new Error('Error getting config: ' + x['error-code'][1])
        }),
      )
      const dependsOn = answer['depends-on'] ?? answer.dependsOn ?? {}
      await this.setDependencies(effects, dependsOn, true)
      return
    }
  }
  private async setDependencies(
    effects: Effects,
    rawDepends: { [x: string]: readonly string[] },
    configuring: boolean,
  ) {
    const storedDependsOn = await dependsOnFile.read().once()
    const requiredDeps = {
      ...Object.fromEntries(
        Object.entries(this.manifest.dependencies ?? {})
          .filter(
            ([k, v]) =>
              (v?.requirement as { type: string } | undefined)?.type ===
              'required',
          )
          .map(x => [x[0], []]) || [],
      ),
    }

    const dependsOn: Record<string, readonly string[]> = configuring
      ? {
          ...requiredDeps,
          ...rawDepends,
        }
      : storedDependsOn
        ? storedDependsOn
        : requiredDeps

    await dependsOnFile.write(effects, dependsOn)

    await effects.setDependencies({
      dependencies: Object.entries(dependsOn).flatMap(
        ([key, value]): T.Dependencies => {
          const dependency = this.manifest.dependencies?.[key]
          if (!dependency) return []
          const versionRange = dependency.version
          const kind = 'running'
          return [
            {
              id: key,
              versionRange,
              kind,
              healthChecks: [...value],
            },
          ]
        },
      ),
    })
  }

  async migration(
    effects: Effects,
    version:
      | { from: VersionRange | ExtendedVersion }
      | { to: VersionRange | ExtendedVersion },
    timeoutMs: number | null,
  ): Promise<{ configured: boolean } | null> {
    let migration
    let args: [string, ...string[]]
    if ('from' in version) {
      if (overlaps(this.version, version.from)) return null
      args = [version.from.toString(), 'from']
      if (!this.manifest.migrations) return { configured: true }
      migration = Object.entries(this.manifest.migrations.from)
        .map(
          ([version, procedure]) =>
            [VersionRange.parseEmver(version), procedure] as const,
        )
        .find(([versionEmver, _]) => overlaps(versionEmver, version.from))
    } else {
      if (overlaps(this.version, version.to)) return null
      args = [version.to.toString(), 'to']
      if (!this.manifest.migrations) return { configured: true }
      migration = Object.entries(this.manifest.migrations.to)
        .map(
          ([version, procedure]) =>
            [VersionRange.parseEmver(version), procedure] as const,
        )
        .find(([versionEmver, _]) => overlaps(versionEmver, version.to))
    }

    if (migration) {
      const [_, procedure] = migration as readonly [unknown, Procedure]
      if (procedure.type === 'docker') {
        const commands = [procedure.entrypoint, ...procedure.args]
        const container = await DockerProcedureContainer.of(
          effects,
          this.manifest.id,
          procedure,
          this.manifest.volumes,
          `Migration - ${commands.join(' ')}`,
        )
        return JSON.parse(
          (
            await container.execFail(commands, timeoutMs, {
              input: JSON.stringify(args[0]),
            })
          ).stdout.toString(),
        )
      } else if (procedure.type === 'script') {
        const moduleCode = await this.moduleCode
        const method = moduleCode.migration
        if (!method)
          throw new Error('Expecting that the method migration exists')
        return (await method(
          polyfillEffects(effects, this.manifest),
          ...args,
        ).then(x => {
          if ('result' in x) return x.result
          if ('error' in x) throw new Error('Error getting config: ' + x.error)
          throw new Error('Error getting config: ' + x['error-code'][1])
        })) as any
      }
    }
    return null
  }
  async properties(
    effects: Effects,
    timeoutMs: number | null,
  ): Promise<PropertiesReturn> {
    const setConfigValue = this.manifest.properties as
      | Procedure
      | null
      | undefined
    if (!setConfigValue) throw new Error('There is no properties')
    if (setConfigValue.type === 'docker') {
      const commands = [setConfigValue.entrypoint, ...setConfigValue.args]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        setConfigValue,
        this.manifest.volumes,
        `Properties - ${commands.join(' ')}`,
      )
      const properties = matchProperties.parse(
        JSON.parse(
          (await container.execFail(commands, timeoutMs)).stdout.toString(),
        ),
      )
      return asProperty(properties.data)
    } else if (setConfigValue.type === 'script') {
      const moduleCode = this.moduleCode
      const method = moduleCode.properties
      if (!method)
        throw new Error('Expecting that the method properties exists')
      const properties = matchProperties.parse(
        await method(polyfillEffects(effects, this.manifest)).then(
          fromReturnType,
        ),
      )
      return asProperty(properties.data)
    }
    throw new Error(`Unknown type in the fetch properties: ${setConfigValue}`)
  }
  async action(
    effects: Effects,
    actionId: string,
    formData: unknown,
    timeoutMs: number | null,
  ): Promise<T.ActionResult> {
    const actionProcedure = this.manifest.actions?.[actionId]
      ?.implementation as Procedure | undefined
    const toActionResult = ({
      message,
      value,
      copyable,
      qr,
    }: U.ActionResult): T.ActionResult => ({
      version: '0',
      message,
      value: value ?? null,
      copyable,
      qr,
    })
    if (!actionProcedure) throw Error('Action not found')
    if (actionProcedure.type === 'docker') {
      const subcontainer = actionProcedure.inject
        ? this.currentRunning?.mainSubContainerHandle
        : undefined

      const env: Record<string, string> = actionProcedure.inject
        ? {
            HOME: '/root',
          }
        : {}
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        actionProcedure,
        this.manifest.volumes,
        `Action ${actionId}`,
        {
          subcontainer,
        },
      )
      return toActionResult(
        JSON.parse(
          (
            await container.execFail(
              [
                actionProcedure.entrypoint,
                ...actionProcedure.args,
                JSON.stringify(formData),
              ],
              timeoutMs,
              { env },
            )
          ).stdout.toString(),
        ),
      )
    } else {
      const moduleCode = await this.moduleCode
      const method = moduleCode.action?.[actionId]
      if (!method) throw new Error('Expecting that the method action exists')
      return await method(
        polyfillEffects(effects, this.manifest),
        formData as any,
      )
        .then(fromReturnType)
        .then(toActionResult)
    }
  }
  async dependenciesCheck(
    effects: Effects,
    id: string,
    oldConfig: unknown,
    timeoutMs: number | null,
  ): Promise<object> {
    const actionProcedure = this.manifest.dependencies?.[id]?.config?.check as
      | Procedure
      | undefined
    if (!actionProcedure) return { message: 'Action not found', value: null }
    if (actionProcedure.type === 'docker') {
      const commands = [
        actionProcedure.entrypoint,
        ...actionProcedure.args,
        JSON.stringify(oldConfig),
      ]
      const container = await DockerProcedureContainer.of(
        effects,
        this.manifest.id,
        actionProcedure,
        this.manifest.volumes,
        `Dependencies Check - ${commands.join(' ')}`,
      )
      return JSON.parse(
        (await container.execFail(commands, timeoutMs)).stdout.toString(),
      )
    } else if (actionProcedure.type === 'script') {
      const moduleCode = await this.moduleCode
      const method = moduleCode.dependencies?.[id]?.check
      if (!method)
        throw new Error(
          `Expecting that the method dependency check ${id} exists`,
        )
      return (await method(
        polyfillEffects(effects, this.manifest),
        oldConfig as any,
      ).then(x => {
        if ('result' in x) return x.result
        if ('error' in x) throw new Error('Error getting config: ' + x.error)
        throw new Error('Error getting config: ' + x['error-code'][1])
      })) as any
    } else {
      return {}
    }
  }
  async dependenciesAutoconfig(
    effects: Effects,
    id: string,
    timeoutMs: number | null,
  ): Promise<void> {
    // TODO: docker
    const status = await getStatus(effects, { packageId: id }).const()
    if (!status) return
    try {
      await effects.mount({
        location: `/media/embassy/${id}`,
        target: {
          packageId: id,
          volumeId: 'embassy',
          subpath: null,
          readonly: true,
          idmap: [],
        },
      })
    } catch (e) {
      console.error(
        `Failed to mount dependency volume for ${id}, skipping autoconfig:`,
        e,
      )
      return
    }
    configFile
      .withPath(`/media/embassy/${id}/config.json`)
      .read()
      .onChange(effects, async (oldConfig: U.Config) => {
        if (!oldConfig) return { cancel: false }
        const moduleCode = await this.moduleCode
        const method = moduleCode?.dependencies?.[id]?.autoConfigure
        if (!method) return { cancel: true }
        const newConfig = (await method(
          polyfillEffects(effects, this.manifest),
          JSON.parse(JSON.stringify(oldConfig)),
        ).then(x => {
          if ('result' in x) return x.result
          if ('error' in x) throw new Error('Error getting config: ' + x.error)
          throw new Error('Error getting config: ' + x['error-code'][1])
        })) as any
        const diff = partialDiff(oldConfig, newConfig)
        if (diff) {
          await effects.action.createTask({
            actionId: 'config',
            packageId: id,
            replayId: `${id}/config`,
            severity: 'important',
            reason: `Configure this dependency for the needs of ${this.manifest.title}`,
            input: {
              kind: 'partial',
              accept: [diff.diff],
              set: diff.diff,
            },
            when: {
              condition: 'input-not-matches',
              once: false,
            },
          })
        }
        return { cancel: false }
      })
  }
}

const matchPointer = z.object({
  type: z.literal('pointer'),
})

const matchPointerPackage = z.object({
  subtype: z.literal('package'),
  target: z.enum(['tor-key', 'tor-address', 'lan-address']),
  'package-id': z.string(),
  interface: z.string(),
})
const matchPointerConfig = z.object({
  subtype: z.literal('package'),
  target: z.enum(['config']),
  'package-id': z.string(),
  selector: z.string(),
  multi: z.boolean(),
})
const matchSpec = z.object({
  spec: z.record(z.string(), z.unknown()),
})
const matchVariants = z.object({ variants: z.record(z.string(), z.unknown()) })
function isMatchPointer(v: unknown): v is z.infer<typeof matchPointer> {
  return matchPointer.safeParse(v).success
}
function isMatchSpec(v: unknown): v is z.infer<typeof matchSpec> {
  return matchSpec.safeParse(v).success
}
function isMatchVariants(v: unknown): v is z.infer<typeof matchVariants> {
  return matchVariants.safeParse(v).success
}
function cleanSpecOfPointers<T>(mutSpec: T): T {
  if (typeof mutSpec !== 'object' || mutSpec === null) return mutSpec
  for (const key in mutSpec) {
    const value = mutSpec[key]
    if (isMatchSpec(value))
      value.spec = cleanSpecOfPointers(value.spec) as Record<string, unknown>
    if (isMatchVariants(value))
      value.variants = Object.fromEntries(
        Object.entries(value.variants).map(([key, value]) => [
          key,
          cleanSpecOfPointers(value),
        ]),
      )
    if (!isMatchPointer(value)) continue
    delete mutSpec[key]
    // // if (value.target === )
  }

  return mutSpec
}
function isKeyOf<O extends object>(
  key: string,
  ofObject: O,
): key is keyof O & string {
  return key in ofObject
}

// prettier-ignore
type CleanConfigFromPointers<C, S> = 
  [C, S] extends [object, object] ? {
    [K in (keyof C & keyof S ) & string]: (
      S[K] extends {type: "pointer"} ? never :
      S[K] extends {spec: object & infer B} ? CleanConfigFromPointers<C[K], B> :
      C[K]
    )
  } :
  null

async function updateConfig(
  effects: Effects,
  manifest: Manifest,
  spec: OldConfigSpec,
  mutConfigValue: Record<string, unknown>,
) {
  for (const key in spec) {
    const specValue = spec[key]

    if (specValue.type === 'object') {
      await updateConfig(
        effects,
        manifest,
        specValue.spec as OldConfigSpec,
        mutConfigValue[key] as Record<string, unknown>,
      )
    } else if (specValue.type === 'list' && specValue.subtype === 'object') {
      const list = mutConfigValue[key] as unknown[]
      for (let val of list) {
        await updateConfig(
          effects,
          manifest,
          { ...(specValue.spec as any), type: 'object' as const },
          val as Record<string, unknown>,
        )
      }
    } else if (specValue.type === 'union') {
      const union = mutConfigValue[key] as Record<string, unknown>
      await updateConfig(
        effects,
        manifest,
        specValue.variants[union[specValue.tag.id] as string] as OldConfigSpec,
        mutConfigValue[key] as Record<string, unknown>,
      )
    } else if (
      specValue.type === 'pointer' &&
      specValue.subtype === 'package'
    ) {
      if (specValue.target === 'config') {
        const jp = require('jsonpath')
        const depId = specValue['package-id']
        const depStatus = await getStatus(effects, { packageId: depId }).const()
        if (!depStatus) {
          mutConfigValue[key] = null
          continue
        }
        await effects.mount({
          location: `/media/embassy/${depId}`,
          target: {
            packageId: depId,
            volumeId: 'embassy',
            subpath: null,
            readonly: true,
            idmap: [],
          },
        })
        const remoteConfig = configFile
          .withPath(`/media/embassy/${depId}/config.json`)
          .read()
          .once()
        console.debug(remoteConfig)
        const configValue = specValue.multi
          ? jp.query(remoteConfig, specValue.selector)
          : jp.query(remoteConfig, specValue.selector, 1)[0]
        mutConfigValue[key] = configValue === undefined ? null : configValue
      } else if (specValue.target === 'tor-key') {
        throw new Error('This service uses an unsupported target TorKey')
      } else {
        const specInterface = specValue.interface
        const serviceInterfaceId = extractServiceInterfaceId(
          manifest,
          specInterface,
        )
        if (!serviceInterfaceId) {
          mutConfigValue[key] = ''
          return
        }
        const filled = await (async () => {
          const serviceInterface = await effects.getServiceInterface({
            packageId: specValue['package-id'],
            serviceInterfaceId,
          })
          if (!serviceInterface) return null
          const host = await effects.getHostInfo({
            packageId: specValue['package-id'],
            hostId: serviceInterface.addressInfo.hostId,
          })
          return {
            ...serviceInterface,
            addressInfo: host
              ? utils.filledAddress(host, serviceInterface.addressInfo)
              : null,
          }
        })().catch(x => {
          console.error('Could not get the service interface', utils.asError(x))
          return null
        })
        const catchFn = <X>(fn: () => X) => {
          try {
            return fn()
          } catch (e) {
            return undefined
          }
        }
        const url: string =
          filled === null || filled.addressInfo === null
            ? ''
            : catchFn(
                () =>
                  filled.addressInfo!.filter({ kind: 'mdns' })!.hostnames[0]
                    .hostname,
              ) || ''
        mutConfigValue[key] = url
      }
    }
  }
}
function extractServiceInterfaceId(manifest: Manifest, specInterface: string) {
  const internalPort =
    Object.entries(
      manifest.interfaces[specInterface]?.['lan-config'] || {},
    )[0]?.[1]?.internal ||
    Object.entries(
      manifest.interfaces[specInterface]?.['tor-config']?.['port-mapping'] ||
        {},
    )?.[0]?.[1]

  if (!internalPort) return null
  const serviceInterfaceId = `${specInterface}-${internalPort}`
  return serviceInterfaceId
}
async function convertToNewConfig(value: OldGetConfigRes) {
  try {
    const valueSpec: OldConfigSpec = matchOldConfigSpec.parse(value.spec)
    const spec = transformConfigSpec(valueSpec)
    if (!value.config) return { spec, config: null }
    const config = transformOldConfigToNew(valueSpec, value.config) ?? null
    return { spec, config }
  } catch (e) {
    console.error(e)
    throw e
  }
}
