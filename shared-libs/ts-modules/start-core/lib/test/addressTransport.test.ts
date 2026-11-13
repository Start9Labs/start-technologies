import type { ProxyOptions } from '../osBindings'
import { addressTransport, transportLabels } from '../util/addressTransport'

const proxy: ProxyOptions = {
  type: 'http',
  addXForwardedHeaders: true,
  auth: null,
}

describe('effective address transport', () => {
  test.each(['tcp', 'udp', 'both'] as const)(
    'raw addresses expose declared %s transport',
    transport => {
      expect(addressTransport({ transport, proxy: null }, { ssl: false })).toBe(
        transport,
      )
    },
  )

  test.each(['tcp', 'both'] as const)(
    'TLS and HTTP proxy addresses expose TCP for a %s binding',
    transport => {
      expect(addressTransport({ transport, proxy: null }, { ssl: true })).toBe(
        'tcp',
      )
      expect(addressTransport({ transport, proxy }, { ssl: false })).toBe('tcp')
      expect(addressTransport({ transport, proxy }, { ssl: true })).toBe('tcp')
    },
  )

  test('an SSL sibling does not narrow the raw address of a Both binding', () => {
    const options = {
      transport: 'both' as const,
      proxy: null,
      addSsl: { preferredExternalPort: 443, alpn: null },
    }
    expect(addressTransport(options, { ssl: false })).toBe('both')
    expect(addressTransport(options, { ssl: true })).toBe('tcp')
  })

  test('labels distinguish TCP, UDP, and both', () => {
    expect(transportLabels).toEqual({ tcp: 'TCP', udp: 'UDP', both: 'TCP/UDP' })
  })
})
