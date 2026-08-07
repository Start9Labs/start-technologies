import { Effects } from '../Effects'

/**
 * This server's root CA certificate, PEM encoded.
 *
 * Every LAN-facing address StartOS advertises — mDNS, LAN IP, private domain —
 * is served with a certificate chaining to this root, and nothing inside a
 * service container trusts it by default. A container that has to reach another
 * service on this server over HTTPS needs this certificate installed, either
 * into its own trust store or via a runtime that takes one (e.g. Node's
 * `NODE_EXTRA_CA_CERTS`).
 *
 * A plain Promise, not reactive: the root is minted once at setup and lives ten
 * years, with no rotation flow.
 */
export async function getRootCa(effects: Effects): Promise<string> {
  // The root only comes back inside a fullchain, so we mint a leaf we discard.
  const chain = await effects.getSslCertificate({ hostnames: [] })
  return chain[chain.length - 1]
}
