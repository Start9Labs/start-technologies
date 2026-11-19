import {
  checkWebUrl,
  runHealthScript,
  timeoutPromise,
} from '../health/checkFns'

beforeEach(() => {
  jest.useFakeTimers()
  jest.spyOn(console, 'warn').mockImplementation(() => {})
  jest.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => {
  const timers = jest.getTimerCount()
  jest.useRealTimers()
  jest.restoreAllMocks()
  expect(timers).toBe(0)
})

test('a caller can retire an unused deadline', () => {
  const deadline = timeoutPromise(30_000)
  expect(jest.getTimerCount()).toBe(1)
  deadline.cancel()
})

test('web checks retire deadlines on response and rejection', async () => {
  const fetch = jest.spyOn(globalThis, 'fetch')
  fetch.mockResolvedValueOnce({} as Response)
  await expect(
    checkWebUrl({} as never, 'https://example.invalid'),
  ).resolves.toMatchObject({ result: 'success' })
  expect(jest.getTimerCount()).toBe(0)
  fetch.mockRejectedValueOnce(new Error('offline'))
  await expect(
    checkWebUrl({} as never, 'https://example.invalid'),
  ).resolves.toMatchObject({ result: 'failure' })
})

test('script checks retire deadlines on completion and rejection', async () => {
  const execFail = jest
    .fn()
    .mockResolvedValueOnce({ stdout: Buffer.from('ready') })
  const subcontainer = { execFail } as never
  await expect(runHealthScript(['probe'], subcontainer)).resolves.toMatchObject(
    { result: 'success' },
  )
  expect(jest.getTimerCount()).toBe(0)
  execFail.mockRejectedValueOnce(new Error('failed'))
  await expect(runHealthScript(['probe'], subcontainer)).rejects.toMatchObject({
    result: 'failure',
  })
})
