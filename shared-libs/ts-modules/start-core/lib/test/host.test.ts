import { Effects } from '../Effects'
import {
  BindOptions,
  BindOptionsByProtocol,
  MultiHost,
} from '../interfaces/Host'
import { ProxyAuth } from '../osBindings'

function fixture() {
  const bind = jest.fn(async () => null)
  const host = new MultiHost({
    effects: { bind } as unknown as Effects,
    id: 'service',
  })
  return { bind, host }
}

describe('MultiHost.bindPort HTTP proxy configuration', () => {
  test.each([
    ['http', 80, null, ['http/1.1']],
    ['https', 443, { ssl: true }, null],
    ['ws', 80, null, ['http/1.1']],
    ['wss', 443, { ssl: true }, null],
  ] as const)(
    '%s identifies HTTP independently of SSL',
    async (protocol, port, secure, alpn) => {
      const { host, bind } = fixture()
      await host.bindPort(8080, { protocol })
      expect(bind).toHaveBeenCalledWith({
        id: 'service',
        internalPort: 8080,
        preferredExternalPort: port,
        secure,
        addSsl: { preferredExternalPort: 443, alpn },
        proxy: { type: 'http', addXForwardedHeaders: true, auth: null },
      })
    },
  )

  test.each(['http', 'https', 'ws', 'wss'] as const)(
    '%s remains HTTP-aware with headers and auth disabled',
    async protocol => {
      const { host, bind } = fixture()
      await host.bindPort(8080, {
        protocol,
        proxy: { addXForwardedHeaders: false, auth: null },
      })
      expect(bind).toHaveBeenCalledWith(
        expect.objectContaining({
          proxy: { type: 'http', addXForwardedHeaders: false, auth: null },
        }),
      )
    },
  )

  test.each(['http', 'https', 'ws', 'wss'] as const)(
    '%s can disable its default proxy without changing TLS defaults',
    async protocol => {
      const { host, bind } = fixture()
      await host.bindPort(8080, { protocol, proxy: null })
      expect(bind).toHaveBeenCalledWith(
        expect.objectContaining({
          proxy: null,
          addSsl: expect.objectContaining({ preferredExternalPort: 443 }),
        }),
      )
    },
  )

  test.each([
    ['http', null, null],
    ['https', { ssl: true }, 'https'],
    ['ws', null, null],
    ['wss', { ssl: true }, 'wss'],
  ] as const)(
    '%s can disable both proxying and TLS termination',
    async (protocol, secure, sslScheme) => {
      const { host, bind } = fixture()
      const origin = await host.bindPort(8080, {
        protocol,
        addSsl: null,
        proxy: null,
      })
      expect(origin.scheme).toBe(protocol)
      expect(origin.sslScheme).toBe(sslScheme)
      expect(bind).toHaveBeenCalledWith(
        expect.objectContaining({
          secure,
          addSsl: null,
          proxy: null,
        }),
      )
    },
  )

  test.each(['ssh', 'dns', null] as const)(
    '%s can opt into HTTP proxying with partial overrides',
    async protocol => {
      const { host, bind } = fixture()
      const auth: ProxyAuth = {
        type: 'bearer',
        tokens: ['secret'],
        realm: null,
      }
      const options =
        protocol === null
          ? { protocol, preferredExternalPort: 8080, secure: null }
          : { protocol }
      await host.bindPort(8080, { ...options, proxy: { auth } })
      expect(bind).toHaveBeenCalledWith(
        expect.objectContaining({
          proxy: { type: 'http', addXForwardedHeaders: true, auth },
        }),
      )
    },
  )

  test('protocol:null can rewrap TLS with explicit HTTP proxy options', async () => {
    const { host, bind } = fixture()
    await host.bindPort(8080, {
      protocol: null,
      preferredExternalPort: 8080,
      secure: { ssl: true },
      addSsl: { preferredExternalPort: 8443, alpn: null },
      proxy: { addXForwardedHeaders: false },
    })
    expect(bind).toHaveBeenCalledWith({
      id: 'service',
      internalPort: 8080,
      preferredExternalPort: 8080,
      secure: { ssl: true },
      addSsl: { preferredExternalPort: 8443, alpn: null },
      proxy: { type: 'http', addXForwardedHeaders: false, auth: null },
    })
  })

  test.each(['http', 'ws'] as const)(
    '%s can configure an HTTP proxy without an SSL wrapper',
    async protocol => {
      const { host, bind } = fixture()
      const auth: ProxyAuth = {
        type: 'bearer',
        tokens: ['secret'],
        realm: null,
      }
      const origin = await host.bindPort(8080, {
        protocol,
        addSsl: null,
        proxy: { auth },
      })
      expect(origin.scheme).toBe(protocol)
      expect(origin.sslScheme).toBeNull()
      expect(bind).toHaveBeenCalledWith(
        expect.objectContaining({
          addSsl: null,
          proxy: { type: 'http', addXForwardedHeaders: true, auth },
        }),
      )
    },
  )

  test.each<ProxyAuth>([
    {
      type: 'basic',
      credentials: [{ username: 'user', password: 'secret' }],
      realm: null,
    },
    { type: 'bearer', tokens: ['secret'], realm: null },
  ])('keeps $type auth separate from SSL options', async auth => {
    const { host, bind } = fixture()
    await host.bindPort(8080, {
      protocol: 'https',
      preferredExternalPort: 8443,
      proxy: { auth },
      addSsl: {
        preferredExternalPort: 9443,
        alpn: ['http/1.1'],
        upstreamCertValidation: 'disable',
      },
    })
    expect(bind).toHaveBeenCalledWith({
      id: 'service',
      internalPort: 8080,
      preferredExternalPort: 8443,
      secure: { ssl: true },
      proxy: { type: 'http', addXForwardedHeaders: true, auth },
      addSsl: {
        preferredExternalPort: 9443,
        alpn: ['http/1.1'],
        upstreamCertValidation: 'disable',
      },
    })
  })

  test.each([
    ['ssh', 22],
    ['dns', 53],
  ] as const)(
    '%s binds raw traffic without HTTP features',
    async (protocol, port) => {
      const { host, bind } = fixture()
      await host.bindPort(8080, { protocol })
      expect(bind).toHaveBeenCalledWith({
        id: 'service',
        internalPort: 8080,
        preferredExternalPort: port,
        secure: { ssl: false },
        addSsl: null,
        proxy: null,
      })
      await host.bindPort(8080, { protocol, addSsl: {} })
      expect(bind).toHaveBeenLastCalledWith({
        id: 'service',
        internalPort: 8080,
        preferredExternalPort: port,
        secure: { ssl: false },
        addSsl: { preferredExternalPort: 443, alpn: null },
        proxy: null,
      })
    },
  )

  test.each([null, { ssl: false }, { ssl: true }])(
    'keeps protocol:null raw with security %j',
    async secure => {
      const { host, bind } = fixture()
      await host.bindPort(8080, {
        protocol: null,
        preferredExternalPort: 8080,
        secure,
        addSsl: null,
      })
      expect(bind).toHaveBeenCalledWith({
        id: 'service',
        internalPort: 8080,
        preferredExternalPort: 8080,
        secure,
        addSsl: null,
        proxy: null,
      })
    },
  )

  test('protocol:null can wrap a non-HTTP port in SSL', async () => {
    const { host, bind } = fixture()
    await host.bindPort(8080, {
      protocol: null,
      preferredExternalPort: 8080,
      secure: null,
      addSsl: { preferredExternalPort: 8443, alpn: null },
    })
    expect(bind).toHaveBeenCalledWith({
      id: 'service',
      internalPort: 8080,
      preferredExternalPort: 8080,
      secure: null,
      addSsl: { preferredExternalPort: 8443, alpn: null },
      proxy: null,
    })
  })

  test('public proxy options accept only HTTP proxy types', () => {
    const accept = (options: BindOptionsByProtocol) => options
    // @ts-expect-error Only the HTTP proxy type is implemented.
    accept({ protocol: 'http', proxy: { type: 'socks' } })
  })

  test('public SSL options reject legacy HTTP fields', () => {
    const accept = (options: BindOptionsByProtocol) => options
    const ssl = { addXForwardedHeaders: false }
    const rawSsl = (options: BindOptions['addSsl']) => options
    const preferredExternalPort = 443
    // @ts-expect-error Proxy auth belongs under proxy.
    accept({ protocol: 'http', addSsl: { auth: null } })
    // @ts-expect-error Forwarded headers belong under proxy.
    accept({ protocol: 'http', addSsl: ssl })
    // @ts-expect-error Raw SSL options have no HTTP auth.
    rawSsl({ preferredExternalPort, auth: null })
    // @ts-expect-error Raw SSL options have no HTTP headers.
    rawSsl({ preferredExternalPort, addXForwardedHeaders: false })
    // @ts-expect-error SSH SSL options have no HTTP headers.
    accept({ protocol: 'ssh', addSsl: ssl })
    // @ts-expect-error DNS SSL options have no HTTP auth.
    accept({ protocol: 'dns', addSsl: { auth: null } })
  })
})
