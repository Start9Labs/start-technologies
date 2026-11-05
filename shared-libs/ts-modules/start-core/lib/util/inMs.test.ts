import { inMs } from './inMs'

describe('inMs', () => {
  test('28.001s', () => {
    expect(inMs('28.001s')).toBe(28001)
  })
  test('28.123s', () => {
    expect(inMs('28.123s')).toBe(28123)
  })
  test('.123s', () => {
    expect(inMs('.123s')).toBe(123)
  })
  test('123ms', () => {
    expect(inMs('123ms')).toBe(123)
  })
  test('1h', () => {
    expect(inMs('1h')).toBe(3600000)
  })
  test('1m', () => {
    expect(inMs('1m')).toBe(60000)
  })
  test('1d', () => {
    expect(inMs('1d')).toBe(1000 * 60 * 60 * 24)
  })
  test('123', () => {
    expect(() => inMs('123')).toThrow('Invalid time format: 123')
  })
  test('123 as number', () => {
    expect(inMs(123)).toBe(123)
  })
  test('undefined', () => {
    expect(inMs(undefined)).toBe(undefined)
  })
})
