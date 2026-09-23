import { effectParams } from './EffectCreator'

describe('effectParams', () => {
  test('tags the call with the calling procedure', () => {
    expect(effectParams({ actionId: 'a' }, 'procedure')).toEqual({
      actionId: 'a',
      eventId: 'procedure',
    })
  })

  test('keeps the event id an action run names', () => {
    expect(
      effectParams({ actionId: 'a', eventId: 'form' }, 'procedure'),
    ).toEqual({ actionId: 'a', eventId: 'form' })
  })

  test('sends no event id outside a procedure', () => {
    expect(JSON.stringify(effectParams({ actionId: 'a' }, null))).toBe(
      '{"actionId":"a"}',
    )
  })
})
