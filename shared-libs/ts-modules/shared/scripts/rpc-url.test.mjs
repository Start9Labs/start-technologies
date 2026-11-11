import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { rpcWebSocketUrl } from '../src/services/rpc-request.ts'

test('RPC WebSocket URLs use the native route and retain the selected authority', () => {
  assert.equal(
    rpcWebSocketUrl('/rpc/v1', 'http://os.test:5959'),
    'ws://os.test:5959/rpc/v2',
  )
  assert.equal(
    rpcWebSocketUrl('/rpc/v1', 'https://os.test'),
    'wss://os.test/rpc/v2',
  )
})

test('continuation URLs encode their GUID and preserve endpoint parameters', () => {
  assert.equal(
    rpcWebSocketUrl(
      'https://other.test/rpc/v1?existing=kept',
      'http://os.test',
      'guid/value?',
    ),
    'wss://other.test/rpc/v2/guid%2Fvalue%3F?existing=kept',
  )
})

test('deployment prefixes are shared by RPC and exact-GUID admission', () => {
  assert.equal(
    rpcWebSocketUrl(
      'https://server.test/nested/rpc/v1/?existing=kept',
      'http://unused',
    ),
    'wss://server.test/nested/rpc/v2?existing=kept',
  )
  assert.equal(
    rpcWebSocketUrl('/nested/rpc/v0', 'https://server.test', 'guid'),
    'wss://server.test/nested/rpc/v2/guid',
  )
})

test('RPC WebSocket URLs preserve IPv6 authority and port', () => {
  assert.equal(
    rpcWebSocketUrl('/rpc/v1', 'http://[2001:db8::1]:5959', 'guid'),
    'ws://[2001:db8::1]:5959/rpc/v2/guid',
  )
})
