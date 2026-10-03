import { Effects } from '../Effects'
import { PackageId } from '../osBindings'
import { Watchable } from './Watchable'

export class GetContainerIp extends Watchable<
  Awaited<ReturnType<Effects['getContainerIp']>>
> {
  protected readonly label = 'GetContainerIp'

  constructor(
    effects: Effects,
    readonly opts: { packageId?: PackageId } = {},
  ) {
    super(effects)
  }

  protected fetch(callback?: () => void) {
    return this.effects.getContainerIp({ ...this.opts, callback })
  }
}
