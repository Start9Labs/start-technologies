import { Effects } from '../Effects'
import { PackageId } from '../osBindings'
import { Watchable } from './Watchable'

/** The calling service's own container always has an IP; another package's may not. */
export type ContainerIp<Opts extends { packageId?: PackageId }> = Opts extends {
  packageId?: undefined
}
  ? string
  : string | null

export class GetContainerIp<
  Opts extends { packageId?: PackageId } = {},
> extends Watchable<ContainerIp<Opts>> {
  protected readonly label = 'GetContainerIp'

  constructor(
    effects: Effects,
    readonly opts: Opts = {} as Opts,
  ) {
    super(effects)
  }

  protected fetch(callback?: () => void) {
    return this.effects.getContainerIp({
      ...this.opts,
      callback,
    }) as Promise<ContainerIp<Opts>>
  }
}
