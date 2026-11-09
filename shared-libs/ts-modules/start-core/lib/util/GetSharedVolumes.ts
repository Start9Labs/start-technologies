import { Effects } from '../Effects'
import { GrantedSharedVolume } from '../osBindings'
import { Watchable } from './Watchable'

/** Reactive grants; source changes invalidate readers even when returned metadata is unchanged. */
export class GetSharedVolumes extends Watchable<GrantedSharedVolume[]> {
  protected readonly label = 'GetSharedVolumes'

  constructor(effects: Effects) {
    super(effects, () => false)
  }

  protected fetch(callback?: () => void): Promise<GrantedSharedVolume[]> {
    return this.effects.getSharedVolumes({ callback })
  }
}

export function getSharedVolumes(effects: Effects): GetSharedVolumes {
  return new GetSharedVolumes(effects)
}
