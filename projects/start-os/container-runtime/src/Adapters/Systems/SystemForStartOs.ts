import { System } from '../../Interfaces/System'
import { Effects } from '../../Models/Effects'
import { ExtendedVersion, T, utils, VersionRange } from '@start9labs/start-sdk'

export const STARTOS_JS_LOCATION = '/usr/lib/startos/package/index.js'

type RunningMain = {
  stop: () => Promise<void>
}

export class SystemForStartOs implements System {
  private runningMain: RunningMain | undefined
  private starting: boolean = false

  static of() {
    return new SystemForStartOs(require(STARTOS_JS_LOCATION))
  }

  constructor(readonly abi: T.ABI) {
    this
  }

  async init(
    effects: Effects,
    kind: 'install' | 'update' | 'restore' | null,
  ): Promise<void> {
    return void (await this.abi.init({ effects, kind }))
  }

  async exit(
    effects: Effects,
    target: ExtendedVersion | VersionRange | null,
    timeoutMs: number | null = null,
  ): Promise<void> {
    await this.stop()
    return void (await this.abi.uninit({ effects, target }))
  }

  async createBackup(
    effects: T.Effects,
    timeoutMs: number | null,
  ): Promise<void> {
    return void (await this.abi.createBackup({
      effects,
    }))
  }
  getActionInput(
    effects: Effects,
    id: string,
    prefill: Record<string, unknown> | null,
    timeoutMs: number | null,
  ): Promise<T.ActionInput | null> {
    const action = this.abi.actions.get(id)
    if (!action) throw new Error(`Action ${id} not found`)
    return action.getInput({ effects, prefill })
  }
  runAction(
    effects: Effects,
    id: string,
    input: unknown,
    timeoutMs: number | null,
  ): Promise<T.ActionResult | null> {
    const action = this.abi.actions.get(id)
    if (!action) throw new Error(`Action ${id} not found`)
    return action.run({ effects, input })
  }

  async start(effects: Effects): Promise<void> {
    try {
      if (this.runningMain || this.starting) return
      this.starting = true
      effects.constRetry = utils.once(() => {
        console.debug('.const() triggered')
        if (effects.isInContext) effects.restart()
      })
      let mainOnTerm: () => Promise<void> | undefined
      const daemons = await (
        await this.abi.main({
          effects,
        })
      ).build()
      this.runningMain = {
        stop: async () => {
          await daemons.term()
        },
      }
    } finally {
      this.starting = false
    }
  }

  async stop(): Promise<void> {
    if (this.runningMain) {
      try {
        await this.runningMain.stop()
      } finally {
        this.runningMain = undefined
      }
    }
  }
}
