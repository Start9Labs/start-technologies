import { polyfillEffects } from './polyfillEffects'
import { DockerProcedureContainer } from './DockerProcedureContainer'
import { SystemForEmbassy } from '.'
import { T, utils } from '@start9labs/start-sdk'
import { Daemon } from '@start9labs/start-sdk/lib/mainFn/Daemon'
import { Effects } from '../../../Models/Effects'
import { off } from 'node:process'
import { CommandController } from '@start9labs/start-sdk/lib/mainFn/CommandController'
import { SDKManifest } from '@start9labs/start-core/types'
import { SubContainer } from '@start9labs/start-sdk/lib/util/SubContainer'

const EMBASSY_HEALTH_INTERVAL = 15 * 1000
/**
 * We wanted something to represent what the main loop is doing, and
 * in this case it used to run the properties, health, and the docker/ js main.
 * Also, this has an ability to clean itself up too if need be.
 */
export class MainLoop {
  private subcontainerHandle?: SubContainer<SDKManifest>
  get mainSubContainerHandle() {
    this.subcontainerHandle =
      this.subcontainerHandle ??
      this.mainEvent?.daemon?.subcontainer ??
      undefined
    return this.subcontainerHandle
  }
  private healthLoops?: {
    name: string
    interval: NodeJS.Timeout
  }[]

  private mainEvent?: {
    daemon: Daemon<SDKManifest>
  }

  private constructor(
    readonly system: SystemForEmbassy,
    readonly effects: Effects,
  ) {}

  static async of(
    system: SystemForEmbassy,
    effects: Effects,
  ): Promise<MainLoop> {
    const res = new MainLoop(system, effects)
    res.healthLoops = res.constructHealthLoops()
    res.mainEvent = await res.constructMainEvent()
    return res
  }

  private async constructMainEvent() {
    const { system, effects } = this
    const currentCommand: [string, ...string[]] = [
      system.manifest.main.entrypoint,
      ...system.manifest.main.args,
    ]

    await this.setupInterfaces(effects)
    await effects.setMainStatus({ status: 'running' })
    const jsMain = (this.system.moduleCode as any)?.jsMain
    if (jsMain) {
      throw new Error('Unreachable')
    }
    const subcontainer = await DockerProcedureContainer.createSubContainer(
      effects,
      this.system.manifest.id,
      this.system.manifest.main,
      this.system.manifest.volumes,
      `Main - ${currentCommand.join(' ')}`,
    )
    const daemon = await Daemon.of()(this.effects, subcontainer, {
      command: currentCommand,
      runAsInit: true,
      env: {
        TINI_SUBREAPER: 'true',
      },
      sigtermTimeout: utils.inMs(this.system.manifest.main['sigterm-timeout']),
    })

    daemon.start()
    return {
      daemon,
    }
  }

  private async setupInterfaces(effects: T.Effects) {
    for (const interfaceId in this.system.manifest.interfaces) {
      const iface = this.system.manifest.interfaces[interfaceId]
      const internalPorts = new Set<number>()
      for (const port of Object.values(
        iface['tor-config']?.['port-mapping'] || {},
      )) {
        internalPorts.add(parseInt(port))
      }
      for (const port of Object.values(iface['lan-config'] || {})) {
        internalPorts.add(port.internal)
      }
      for (const internalPort of internalPorts) {
        const torConf = Object.entries(
          iface['tor-config']?.['port-mapping'] || {},
        )
          .map(([external, internal]) => ({
            internal: parseInt(internal),
            external: parseInt(external),
          }))
          .find(conf => conf.internal == internalPort)
        const lanConf = Object.entries(iface['lan-config'] || {})
          .map(([external, conf]) => ({
            external: parseInt(external),
            ...conf,
          }))
          .find(conf => conf.internal == internalPort)
        await effects.bind({
          id: interfaceId,
          internalPort,
          preferredExternalPort: torConf?.external || internalPort,
          secure: null,
          addSsl: lanConf?.ssl
            ? {
                preferredExternalPort: lanConf.external,
                alpn: { specified: ['http/1.1'] },
                addXForwardedHeaders: false,
                auth: null,
              }
            : null,
        })
      }
    }
  }

  public async clean(options?: { timeout?: number }) {
    const { mainEvent, healthLoops } = this
    const main = await mainEvent
    delete this.mainEvent
    delete this.healthLoops
    await main?.daemon
      .term()
      .catch((e: unknown) => console.error(`Main loop error`, utils.asError(e)))
    this.effects.setMainStatus({ status: 'stopped' })
    if (healthLoops) healthLoops.forEach(x => clearInterval(x.interval))
  }

  private constructHealthLoops() {
    const { manifest } = this.system
    const effects = this.effects
    const start = Date.now()
    return Object.entries(manifest['health-checks']).map(
      ([healthId, value]) => {
        effects
          .setHealth({
            id: healthId,
            name: value.name,
            result: 'starting',
            message: null,
          })
          .catch(e => console.error(utils.asError(e)))
        const interval = setInterval(async () => {
          const actionProcedure = value
          const timeChanged = Date.now() - start
          if (actionProcedure.type === 'docker') {
            const subcontainer = actionProcedure.inject
              ? this.mainSubContainerHandle
              : undefined
            const commands = [
              actionProcedure.entrypoint,
              ...actionProcedure.args,
            ]
            const container = await DockerProcedureContainer.of(
              effects,
              manifest.id,
              actionProcedure,
              manifest.volumes,
              `Health Check - ${commands.join(' ')}`,
              {
                subcontainer,
              },
            )
            const env: Record<string, string> = actionProcedure.inject
              ? {
                  HOME: '/root',
                }
              : {}
            const executed = await container.exec(commands, {
              input: JSON.stringify(timeChanged),
              env,
            })

            if (executed.exitCode === 0) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'success',
                message: actionProcedure['success-message'] ?? null,
              })
              return
            }
            if (executed.exitCode === 59) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'disabled',
                message:
                  executed.stderr.toString() || executed.stdout.toString(),
              })
              return
            }
            if (executed.exitCode === 60) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'starting',
                message:
                  executed.stderr.toString() || executed.stdout.toString(),
              })
              return
            }
            if (executed.exitCode === 61) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'loading',
                message:
                  executed.stderr.toString() || executed.stdout.toString(),
              })
              return
            }
            const errorMessage = executed.stderr.toString()
            const message = executed.stdout.toString()
            if (!!errorMessage) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'failure',
                message: errorMessage,
              })
              return
            }
            if (executed.exitCode && executed.exitCode > 0) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'failure',
                message:
                  executed.stderr.toString() ||
                  executed.stdout.toString() ||
                  `Program exited with code ${executed.exitCode}:`,
              })
              return
            }
            await effects.setHealth({
              id: healthId,
              name: value.name,
              result: 'success',
              message,
            })
            return
          } else {
            actionProcedure
            const moduleCode = await this.system.moduleCode
            const method = moduleCode.health?.[healthId]
            if (!method) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'failure',
                message: `Expecting that the js health check ${healthId} exists`,
              })
              return
            }

            const result = await method(
              polyfillEffects(effects, this.system.manifest),
              timeChanged,
            )

            if ('result' in result) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'success',
                message: null,
              })
              return
            }
            if ('error' in result) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'failure',
                message: result.error,
              })
              return
            }
            if (!('error-code' in result)) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'failure',
                message: `Unknown error type ${JSON.stringify(result)}`,
              })
              return
            }
            const [code, message] = result['error-code']
            if (code === 59) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'disabled',
                message,
              })
              return
            }
            if (code === 60) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'starting',
                message,
              })
              return
            }
            if (code === 61) {
              await effects.setHealth({
                id: healthId,
                name: value.name,
                result: 'loading',
                message,
              })
              return
            }

            await effects.setHealth({
              id: healthId,
              name: value.name,
              result: 'failure',
              message: `${result['error-code'][0]}: ${result['error-code'][1]}`,
            })
            return
          }
        }, EMBASSY_HEALTH_INTERVAL)

        return { name: healthId, interval }
      },
    )
  }
}
