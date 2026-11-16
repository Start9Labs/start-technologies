import type { ProxyOptions, Transport } from '../osBindings'
import { addressTransport, transportLabel } from '../util/addressTransport'

const proxy: ProxyOptions = {
  type: 'http',
  addXForwardedHeaders: true,
  auth: null,
}
const selections: Transport[] = [['tcp'], ['udp'], ['tcp', 'udp']]

describe('effective address transport', () => {
  test.each(selections.map(transport => ({ transport })))(
    'raw addresses expose declared $transport protocols',
    ({ transport }) => {
      expect(
        addressTransport({ transport, proxy: null }, { ssl: false }),
      ).toEqual(transport)
    },
  )

  test.each([
    { transport: ['tcp'] as Transport },
    { transport: ['tcp', 'udp'] as Transport },
  ])(
    'TLS and HTTP proxy addresses expose TCP for $transport',
    ({ transport }) => {
      expect(
        addressTransport({ transport, proxy: null }, { ssl: true }),
      ).toEqual(['tcp'])
      expect(addressTransport({ transport, proxy }, { ssl: false })).toEqual([
        'tcp',
      ])
      expect(addressTransport({ transport, proxy }, { ssl: true })).toEqual([
        'tcp',
      ])
    },
  )

  test('an SSL sibling does not narrow the raw dual-protocol address', () => {
    const options = {
      transport: ['tcp', 'udp'] as Transport,
      proxy: null,
      addSsl: { preferredExternalPort: 443, alpn: null },
    }
    expect(addressTransport(options, { ssl: false })).toEqual(['tcp', 'udp'])
    expect(addressTransport(options, { ssl: true })).toEqual(['tcp'])
  })

  test('labels distinguish TCP, UDP, and their combination', () => {
    expect(selections.map(transportLabel)).toEqual(['TCP', 'UDP', 'TCP/UDP'])
  })
})
