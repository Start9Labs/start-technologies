import { Api } from '../osBindings/rpc/unlock.schemas'

test('pre-unlock handlers expose only state and password unlock', () => {
  expect(Object.keys(Api._CHILDREN).sort()).toEqual(['state', 'unlock'])
})

test('unlock requires an explicit password and preserves its whitespace', () => {
  const params = Api._CHILDREN.unlock._PARAMS
  expect(params.safeParse({}).success).toBe(false)
  expect(params.parse({ password: '  exact secret  ' })).toEqual({
    password: '  exact secret  ',
  })
})
