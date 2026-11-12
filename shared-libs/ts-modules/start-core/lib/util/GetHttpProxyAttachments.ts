import { Effects } from '../Effects'
import { HttpProxyAttachment } from '../osBindings'
import { Watchable } from './Watchable'

/** Watches this provider's binding/gateway/profile assignments and Unix socket endpoints. */
export class GetHttpProxyAttachments extends Watchable<HttpProxyAttachment[]> {
  protected readonly label = 'GetHttpProxyAttachments'

  protected fetch(callback?: () => void): Promise<HttpProxyAttachment[]> {
    return this.effects.plugin.httpProxy.getAttachments({ callback })
  }
}
