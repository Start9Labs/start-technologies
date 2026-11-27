import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(new Map())
const { systemDefaultGateways } =
  await import('src/app/services/gateway.service')

function gateway(name, v4, v6, deviceType = 'ethernet') {
  return {
    name,
    ipInfo: { defaultRouteV4: v4, defaultRouteV6: v6, deviceType, wanIp: null },
  }
}

test('system default gateways preserve separate IPv4 and IPv6 routes', () => {
  const routes = systemDefaultGateways({
    eth0: gateway('Home', 100, 600),
    wireguard2: gateway('StartTunnel', 600, 100),
    wlan0: gateway('Wi-Fi', 200, null),
  })
  assert.deepEqual(
    routes.map(({ family, id, name }) => ({ family, id, name })),
    [
      { family: 'IPv4', id: 'eth0', name: 'Home' },
      { family: 'IPv6', id: 'wireguard2', name: 'StartTunnel' },
    ],
  )
})

test('system default gateways retain equal-priority routes and exclude internal devices', () => {
  const routes = systemDefaultGateways({
    eth0: gateway('Ethernet', 100, null),
    wlan0: gateway('Wi-Fi', 100, null),
    lo: gateway('Loopback', 0, 0, 'loopback'),
    bridge: gateway('Bridge', 0, 0, 'bridge'),
    disconnected: { name: 'Disconnected', ipInfo: null },
  })
  assert.deepEqual(
    routes.map(route => route.id),
    ['eth0', 'wlan0'],
  )
})

test('system default gateways do not invent a route for a connected interface', () => {
  assert.deepEqual(
    systemDefaultGateways({ eth0: gateway('Home', null, null) }),
    [],
  )
})
