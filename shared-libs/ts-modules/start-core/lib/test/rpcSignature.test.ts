import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { rpcSigningBody } from '../auth/rpc'

const fixtures: { name: string; input: string; body_hex: string }[] =
  JSON.parse(
    readFileSync(
      join(__dirname, '../auth/fixtures/rpc-canonical.json'),
      'utf8',
    ),
  )

test.each(fixtures)('$name', ({ input, body_hex }) => {
  expect(Buffer.from(rpcSigningBody(input)).toString('hex')).toBe(body_hex)
})

for (const input of [
  'null',
  '[]',
  '1',
  '{"method":"test","params":[1e400]}',
  '{"method":"\\ud800"}',
  '{"method":"test","params":{"\\udfff":1}}',
]) {
  test(`rejects a request outside the native JSON object domain: ${input}`, () => {
    expect(() => rpcSigningBody(input)).toThrow()
  })
}
