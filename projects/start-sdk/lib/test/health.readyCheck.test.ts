import * as fs from 'node:fs/promises'
import {
  checkPortListening,
  containsAddress,
} from '../health/checkFns/checkPortListening'

jest.mock('node:fs/promises', () => ({ readFile: jest.fn() }))

const header =
  '  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n'
// Port 50001 (0xC351): a connection in TIME_WAIT and one ESTABLISHED, nothing listening.
const leftoverConnections =
  header +
  '   0: 0B03000A:C351 0103000A:D431 06 00000000:00000000 03:00001234 00000000     0        0 0 3 0000000000000000\n' +
  '   1: 0B03000A:C351 0103000A:D432 01 00000000:00000000 00:00000000 00000000     0        0 21634490 1 0000000000000000 20 4 30 10 -1\n'
const listening =
  header +
  '   0: 00000000:C351 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 21634478 1 0000000000000000 100 0 0 10 0\n'

describe('Health ready check', () => {
  it('Should be able to parse an example information', () => {
    let input = `
  
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode                                                     
   0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 21634478 1 0000000000000000 100 0 0 10 0                  
   1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 21634477 1 0000000000000000 100 0 0 10 0                  
   2: 0B00007F:9671 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 21635458 1 0000000000000000 100 0 0 10 0                  
   3: 00000000:0D73 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 21634479 1 0000000000000000 100 0 0 10 0   
  `

    expect(containsAddress(input, 80)).toBe(true)
    expect(containsAddress(input, 1234)).toBe(false)
  })

  it('matches a socket in the given state only', () => {
    const TCP_LISTEN = 0x0a
    expect(containsAddress(leftoverConnections, 50001)).toBe(true)
    expect(
      containsAddress(leftoverConnections, 50001, undefined, TCP_LISTEN),
    ).toBe(false)
    expect(containsAddress(listening, 50001, undefined, TCP_LISTEN)).toBe(true)
  })
})

describe('checkPortListening', () => {
  const readFile = fs.readFile as unknown as jest.Mock
  const options = { successMessage: 'listening', errorMessage: 'not listening' }
  const procNet = (tcp: string) => (path: string) =>
    Promise.resolve(path === '/proc/net/tcp' ? tcp : header)

  afterEach(() => readFile.mockReset())

  it('is not satisfied by connections left behind after the listener exits', async () => {
    readFile.mockImplementation(procNet(leftoverConnections))
    const result = await checkPortListening({} as never, 50001, options)
    expect(result).toEqual({ result: 'failure', message: 'not listening' })
  })

  it('is satisfied by a listening socket', async () => {
    readFile.mockImplementation(procNet(listening))
    const result = await checkPortListening({} as never, 50001, options)
    expect(result).toEqual({ result: 'success', message: 'listening' })
  })
})
