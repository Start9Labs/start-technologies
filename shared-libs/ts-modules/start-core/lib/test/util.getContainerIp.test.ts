import { Effects } from '../Effects'
import { GetContainerIp } from '../util/GetContainerIp'

describe('GetContainerIp', () => {
  test("reads the calling service's own address without a package id", async () => {
    const getContainerIp = jest.fn(async () => '172.16.0.3')
    const effects = { getContainerIp } as unknown as Effects
    const ip: string = await new GetContainerIp(effects).once()
    expect(ip).toBe('172.16.0.3')
    expect(getContainerIp).toHaveBeenCalledWith({ callback: undefined })
  })
})
