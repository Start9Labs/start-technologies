import { ServiceInterfaceBuilder } from '@start9labs/start-core/interfaces/ServiceInterfaceBuilder'
import { Effects } from '@start9labs/start-core/Effects'
import { MAX_BIND_PORT_RANGE_SIZE } from '@start9labs/start-core/interfaces/Host'
import { sdk } from '../test/output.sdk'

describe('host', () => {
  test('Testing that the types work', () => {
    async function test(effects: Effects) {
      const foo = sdk.MultiHost.of(effects, 'foo')
      const fooOrigin = await foo.bindPort(80, {
        protocol: 'http' as const,
        preferredExternalPort: 80,
      })
      const fooInterface = new ServiceInterfaceBuilder({
        effects,
        name: 'Foo',
        id: 'foo',
        description: 'A Foo',
        type: 'ui',
        username: 'bar',
        path: '/baz',
        query: { qux: 'yes' },
        schemeOverride: null,
        masked: false,
      })

      await fooOrigin.export([fooInterface])
    }
  })

  test('host.get returns interfaces whose addressInfo is pre-filled', () => {
    async function _typecheck(effects: Effects) {
      const host = await sdk.host.getOwn(effects, 'ui').const()
      const ui = Object.values(host?.bindings ?? {})
        .flatMap(b => Object.values(b.interfaces))
        .find(i => i.id === 'ui')

      const urls: string[] = ui?.addressInfo.format('urlstring') ?? []
      const domains =
        ui?.addressInfo.filter({ kind: 'domain' }).format('url') ?? []
      void urls
      void domains
    }
    void _typecheck
  })

  describe('MultiHost.bindPortRange', () => {
    const fakeEffects = (
      bindRange: jest.Mock = jest.fn(async () => null),
    ): Effects => ({ bindRange }) as unknown as Effects

    test('forwards the call to effects.bindRange with normalised params', async () => {
      const bindRange = jest.fn(async () => null)
      const host = sdk.MultiHost.of(fakeEffects(bindRange), 'turn')
      await host.bindPortRange({
        internalStartPort: 49152,
        externalStartPort: 49152,
        numberOfPorts: 100,
      })
      expect(bindRange).toHaveBeenCalledWith({
        id: 'turn',
        internalStartPort: 49152,
        externalStartPort: 49152,
        numberOfPorts: 100,
      })
    })

    test('accepts an offset range (internal !== external)', async () => {
      const bindRange = jest.fn(async () => null)
      const host = sdk.MultiHost.of(fakeEffects(bindRange), 'turn')
      await host.bindPortRange({
        internalStartPort: 49152,
        externalStartPort: 50000,
        numberOfPorts: 10,
      })
      expect(bindRange).toHaveBeenCalledWith({
        id: 'turn',
        internalStartPort: 49152,
        externalStartPort: 50000,
        numberOfPorts: 10,
      })
    })

    test('rejects a single-port range (use bindPort instead)', async () => {
      const host = sdk.MultiHost.of(fakeEffects(), 'turn')
      await expect(
        host.bindPortRange({
          internalStartPort: 49152,
          externalStartPort: 49152,
          numberOfPorts: 1,
        }),
      ).rejects.toThrow(/>= 2/)
    })

    test('rejects numberOfPorts past the SDK cap', async () => {
      const host = sdk.MultiHost.of(fakeEffects(), 'turn')
      await expect(
        host.bindPortRange({
          internalStartPort: 49152,
          externalStartPort: 49152,
          numberOfPorts: MAX_BIND_PORT_RANGE_SIZE + 1,
        }),
      ).rejects.toThrow(/exceeds maximum/)
    })

    test('rejects ranges that walk past port 65535', async () => {
      const host = sdk.MultiHost.of(fakeEffects(), 'turn')
      await expect(
        host.bindPortRange({
          internalStartPort: 65500,
          externalStartPort: 65500,
          numberOfPorts: 100,
        }),
      ).rejects.toThrow(/out of bounds/)
    })

    test('rejects privileged/system start ports (<= 1024)', async () => {
      const host = sdk.MultiHost.of(fakeEffects(), 'turn')
      await expect(
        host.bindPortRange({
          internalStartPort: 1024,
          externalStartPort: 1024,
          numberOfPorts: 10,
        }),
      ).rejects.toThrow(/greater than 1024/)
    })

    test('export() registers the restricted range interface', async () => {
      const bindRange = jest.fn(async () => null)
      const exportRangeServiceInterface = jest.fn(async () => null)
      const effects = {
        bindRange,
        exportRangeServiceInterface,
      } as unknown as Effects
      const range = await sdk.MultiHost.of(effects, 'zmq').bindPortRange({
        internalStartPort: 28332,
        externalStartPort: 28332,
        numberOfPorts: 2,
      })
      await range.export(
        sdk.createRangeInterface(effects, {
          id: 'zmq',
          name: 'ZMQ',
          description: 'Bitcoin ZMQ endpoints',
          scheme: 'tcp',
        }),
      )
      expect(exportRangeServiceInterface).toHaveBeenCalledWith({
        hostId: 'zmq',
        internalStartPort: 28332,
        id: 'zmq',
        name: 'ZMQ',
        description: 'Bitcoin ZMQ endpoints',
        scheme: 'tcp',
      })
    })

    test('export() defaults an omitted scheme to null', async () => {
      const effects = {
        bindRange: jest.fn(async () => null),
        exportRangeServiceInterface: jest.fn(async () => null),
      } as unknown as Effects
      const range = await sdk.MultiHost.of(effects, 'turn').bindPortRange({
        internalStartPort: 49152,
        externalStartPort: 49152,
        numberOfPorts: 100,
      })
      await range.export(
        sdk.createRangeInterface(effects, {
          id: 'turn-relay',
          name: 'TURN Relay',
          description: 'WebRTC media relay ports',
        }),
      )
      expect(effects.exportRangeServiceInterface).toHaveBeenCalledWith(
        expect.objectContaining({ scheme: null }),
      )
    })
  })
})
