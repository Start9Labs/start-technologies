import * as fs from 'fs/promises'
import { types as T } from '@start9labs/start-sdk'
import { SubContainer as SubContainerNS } from '@start9labs/start-sdk/lib/util/SubContainer'
import { DockerProcedure } from '../../../Models/DockerProcedure'
import { DockerProcedureContainer } from './DockerProcedureContainer'
import { Volume } from './matchVolume'

jest.mock('fs/promises', () => ({
  mkdir: jest.fn(),
  writeFile: jest.fn(),
}))

jest.mock('@start9labs/start-sdk/lib/util/SubContainer', () => ({
  SubContainer: {
    eager: jest.fn(),
  },
}))

describe('DockerProcedureContainer', () => {
  test('retains legacy onion certificate names while excluding plaintext addresses', async () => {
    const getSslCertificate = jest.fn().mockResolvedValue(['certificate'])
    const getSslKey = jest.fn().mockResolvedValue('key')
    const effects = {
      getHostInfo: jest.fn().mockResolvedValue({
        bindings: {
          '80': {
            addresses: {
              available: [
                { hostname: 'server.local', ssl: false },
                { hostname: '198.51.100.12', ssl: false },
                { hostname: 'legacy-service.onion', ssl: false },
                { hostname: 'server.local', ssl: true },
              ],
            },
          },
          '443': {
            addresses: {
              available: [
                { hostname: 'server.local', ssl: true },
                { hostname: 'service.example.com', ssl: true },
              ],
            },
          },
        },
      }),
      getSslCertificate,
      getSslKey,
    } as unknown as T.Effects
    const subcontainer = {
      rootfs: '/rootfs',
      mount: jest.fn(),
    }
    jest.mocked(SubContainerNS.eager).mockResolvedValue(subcontainer as never)

    const procedure = {
      type: 'docker',
      image: 'main',
      entrypoint: '/bin/start',
      args: [],
      mounts: { cert: '/certs' },
      'sigterm-timeout': 30,
      inject: false,
    } satisfies DockerProcedure
    const volumes = {
      cert: { type: 'certificate', 'interface-id': 'main' },
    } satisfies Record<string, Volume>

    await DockerProcedureContainer.createSubContainer(
      effects,
      'legacy-service',
      procedure,
      volumes,
      'main',
    )

    const hostnames = [
      'legacy-service.embassy',
      'legacy-service.onion',
      'server.local',
      'service.example.com',
    ]
    expect(getSslCertificate).toHaveBeenCalledWith({ hostnames })
    expect(getSslKey).toHaveBeenCalledWith({ hostnames })
    expect(fs.writeFile).toHaveBeenCalledTimes(2)
  })
})
