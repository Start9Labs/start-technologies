import { Effects } from '../Effects'
import { Watchable } from './Watchable'

/** The calling service's own container address, fixed for the installation. */
export class GetContainerIp extends Watchable<string> {
  protected readonly label = 'GetContainerIp'

  constructor(effects: Effects) {
    super(effects)
  }

  protected fetch(callback?: () => void) {
    return this.effects.getContainerIp({ callback })
  }
}
