import { Effects } from '../Effects'
import { Watchable } from './Watchable'

export class GetSslCertificate extends Watchable<
  Awaited<ReturnType<Effects['getSslCertificate']>>
> {
  protected readonly label = 'GetSslCertificate'

  constructor(
    effects: Effects,
    readonly opts: {
      hostnames: string[]
      algorithm?: 'ecdsa' | 'ed25519'
    },
  ) {
    super(effects)
  }

  protected fetch(callback?: () => void) {
    return this.effects.getSslCertificate({ ...this.opts, callback })
  }
}
