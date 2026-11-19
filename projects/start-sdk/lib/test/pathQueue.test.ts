import { PathQueue } from '../util/PathQueue'

test('same-path work remains ordered after rejection while other paths proceed', async () => {
  const queue = new PathQueue()
  const events: string[] = []
  let release!: () => void
  const barrier = new Promise<void>(resolve => (release = resolve))
  const first = queue.run('/one', async () => {
    events.push('first')
    await barrier
    throw new Error('failed')
  })
  const failure = expect(first).rejects.toThrow('failed')
  const second = queue.run('/one', async () => events.push('second'))
  await queue.run('/two', async () => events.push('other'))
  expect(events).toEqual(['first', 'other'])
  release()
  await failure
  await second
  expect(events).toEqual(['first', 'other', 'second'])
  await expect(queue.run('/one', async () => 'ready')).resolves.toBe('ready')
})
