import { Effects } from '../Effects'
import { HttpProxyAttachment } from '../osBindings'
import { GetHttpProxyAttachments } from '../util/GetHttpProxyAttachments'

const attachment: HttpProxyAttachment = {
  binding: { packageId: 'app', hostId: 'web', internalPort: 8080 },
  profileId: 'standard',
  ingress: { transport: 'unix', path: '/media/startos/http-proxy/in/a.sock' },
  next: { transport: 'unix', path: '/media/startos/http-proxy/next/a.sock' },
  protocols: ['http1'],
}

function fixture() {
  let current = [attachment]
  let changed: (() => void) | undefined
  const fetch = jest.fn(async ({ callback }: { callback?: () => void }) => {
    changed = callback
    return current
  })
  const effects = {
    isInContext: true,
    onLeaveContext: jest.fn(),
    plugin: { httpProxy: { getAttachments: fetch } },
  } as unknown as Effects
  return {
    effects,
    reader: new GetHttpProxyAttachments(effects),
    fetch,
    update(value: HttpProxyAttachment[]) {
      current = value
      changed?.()
    },
  }
}

test('once reads attachments without registering a callback', async () => {
  const { reader, fetch } = fixture()
  expect(await reader.once()).toEqual([attachment])
  expect(fetch).toHaveBeenCalledWith({ callback: undefined })
})

test('watch emits attachment changes through the standard callback subscription', async () => {
  const { reader, update } = fixture()
  const values = reader.watch()
  expect((await values.next()).value).toEqual([attachment])
  const next = values.next()
  update([])
  expect((await next).value).toEqual([])
  await values.return(undefined as never)
})

test('const invalidates the calling context when its assignments change', async () => {
  const { effects, reader, update } = fixture()
  let retried!: () => void
  const retry = new Promise<void>(resolve => {
    retried = resolve
  })
  effects.constRetry = jest.fn(() => retried())
  expect(await reader.const()).toEqual([attachment])
  update([])
  await retry
  expect(effects.constRetry).toHaveBeenCalledTimes(1)
})
