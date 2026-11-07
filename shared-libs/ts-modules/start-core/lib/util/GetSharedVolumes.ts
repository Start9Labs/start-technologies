import { Effects } from '../Effects'
import { GrantedSharedVolume } from '../osBindings'
import { Watchable } from './Watchable'

/** Reactive user grants, identified by stable IDs rather than editable names. */
export class GetSharedVolumes extends Watchable<GrantedSharedVolume[]> {
  protected readonly label = 'GetSharedVolumes'

  protected fetch(callback?: () => void): Promise<GrantedSharedVolume[]> {
    return this.effects.getSharedVolumes({ callback })
  }
}

export function getSharedVolumes(effects: Effects): GetSharedVolumes {
  return new GetSharedVolumes(effects)
}
