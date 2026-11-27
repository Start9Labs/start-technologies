import assert from 'node:assert/strict'
import test from 'node:test'
import { byIp, entriesByIp, entriesByKey } from '../src/app/utils/order.ts'

test('addresses and subnets order numerically, not as text', () => {
  assert.deepEqual(['10.59.0.10', '10.59.0.2', '10.59.0.1'].sort(byIp), [
    '10.59.0.1',
    '10.59.0.2',
    '10.59.0.10',
  ])
  assert.deepEqual(
    entriesByIp({ '10.59.10.0/24': 'b', '10.59.2.0/24': 'a' }).map(([k]) => k),
    ['10.59.2.0/24', '10.59.10.0/24'],
  )
})

test('gateway keys follow the backend map order whatever the object order', () => {
  assert.deepEqual(
    entriesByKey({ wg0: 1, eth1: 2, Eth0: 3 }).map(([k]) => k),
    ['Eth0', 'eth1', 'wg0'],
  )
})
