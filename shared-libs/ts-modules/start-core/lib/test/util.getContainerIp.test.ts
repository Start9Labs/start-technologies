import { Effects } from '../Effects'
import { GetContainerIp } from '../util/GetContainerIp'

const effects = {
  getContainerIp: jest.fn(async ({ packageId }) =>
    packageId ? null : '10.0.0.2',
  ),
} as unknown as Effects

describe('GetContainerIp', () => {
  test('own-container reads are non-null', async () => {
    const ip: string = await new GetContainerIp(effects).once()
    const empty: string = await new GetContainerIp(effects, {}).once()
    const undefinedId: string = await new GetContainerIp(effects, {
      packageId: undefined,
    }).once()
    expect([ip, empty, undefinedId]).toEqual([
      '10.0.0.2',
      '10.0.0.2',
      '10.0.0.2',
    ])
  })

  test('an unavailable dependency returns null', async () => {
    const ip = await new GetContainerIp(effects, {
      packageId: 'dependency',
    }).once()
    // @ts-expect-error An unavailable dependency has no container IP.
    const nonNull: string = ip
    expect(nonNull).toBeNull()
  })

  test('optional package IDs retain nullability', async () => {
    const options: { packageId?: string } = { packageId: 'dependency' }
    const ip = await new GetContainerIp(effects, options).once()
    // @ts-expect-error Optional package IDs can target an unavailable dependency.
    const nonNull: string = ip
    expect(nonNull).toBeNull()
  })

  test('a possibly undefined package ID retains nullability', async () => {
    const options: { packageId: string | undefined } = {
      packageId: 'dependency',
    }
    const ip = await new GetContainerIp(effects, options).once()
    // @ts-expect-error A supplied package ID can target an unavailable dependency.
    const nonNull: string = ip
    expect(nonNull).toBeNull()
  })
})
