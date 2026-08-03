import { Effects } from '../Effects'
import { Watchable } from './Watchable'

/**
 * Reactive reader for this server's root CA certificate, PEM encoded.
 *
 * Every LAN-facing address StartOS advertises — mDNS, LAN IP, private domain —
 * is served with a certificate chaining to this root, and nothing inside a
 * service container trusts it by default. A container that has to reach another
 * service on this server over HTTPS needs this certificate installed, either
 * into its own trust store or via a runtime that takes one (e.g. Node's
 * `NODE_EXTRA_CA_CERTS`).
 */
export class GetRootCa extends Watchable<string> {
  protected readonly label = 'GetRootCa'

  // The OS exposes the root only as the last link of a fullchain, so obtaining
  // it means asking for a leaf we discard. The OS's own bridge address is the
  // one hostname guaranteed to be issuable: it is the sole entry admitted by a
  // compiled-in subnet check rather than by whatever the gateway table happens
  // to hold. Collapses to a single call if the OS ever exposes the root direct.
  protected async fetch(callback?: () => void) {
    const [, , rootCa] = await this.effects.getSslCertificate({
      hostnames: [await this.effects.getOsIp()],
      callback,
    })
    return rootCa
  }
}
