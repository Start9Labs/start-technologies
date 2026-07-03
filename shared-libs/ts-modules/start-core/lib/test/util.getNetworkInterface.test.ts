import { getHostname } from '../util/filledAddress'

describe('getHostname ', () => {
  const inputToExpected = [
    ['http://localhost:3000', 'localhost'],
    ['http://localhost', 'localhost'],
    ['localhost', 'localhost'],
    ['http://127.0.0.1/', '127.0.0.1'],
    ['http://127.0.0.1/testing/1234?314345', '127.0.0.1'],
    ['127.0.0.1/', '127.0.0.1'],
    ['http://mail.google.com/', 'mail.google.com'],
    ['mail.google.com/', 'mail.google.com'],
  ]

  for (const [input, expectValue] of inputToExpected) {
    test(`should return ${expectValue} for ${input}`, () => {
      expect(getHostname(input)).toEqual(expectValue)
    })
  }
})
