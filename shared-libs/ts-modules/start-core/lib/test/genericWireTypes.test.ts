import { T } from '../index'

describe('public generic wire projections', () => {
  test('backup passwords accept arbitrary caller types', () => {
    const source: T.RecoverySource<{ secret: Uint8Array }> = {
      type: 'backup',
      target: { type: 'disk', logicalname: '/dev/sda' },
      password: { secret: new Uint8Array([1]) },
      serverId: 'server',
    }
    expect(source.password.secret).toEqual(new Uint8Array([1]))
  })

  test('migration remains independent of the password type', () => {
    const source: T.RecoverySource<never> = { type: 'migrate', guid: 'guid' }
    const wire: Extract<T.RecoverySource<string>, { type: 'migrate' }> = source
    expect(wire.guid).toBe('guid')
    // @ts-expect-error Migration has no password field.
    expect(source.password).toBeUndefined()
    // @ts-expect-error Migration GUIDs remain strings.
    const invalid: T.RecoverySource<never> = { type: 'migrate', guid: 1 }
    expect(invalid).toEqual({ type: 'migrate', guid: 1 })
  })

  test('the discriminator narrows both recovery branches', () => {
    function value(source: T.RecoverySource<number>): string | number {
      if (source.type === 'backup') {
        const password: number = source.password
        // @ts-expect-error Backup sources have no migration GUID.
        expect(source.guid).toBeUndefined()
        return password
      }
      const guid: string = source.guid
      // @ts-expect-error Migration sources have no backup target.
      expect(source.target).toBeUndefined()
      return guid
    }
    expect(value({ type: 'migrate', guid: 'guid' })).toBe('guid')
    expect(
      value({
        type: 'backup',
        target: { type: 'disk', logicalname: '/dev/sda' },
        password: 42,
        serverId: 'server',
      }),
    ).toBe(42)
  })

  test('backup fields retain the generated owner types', () => {
    const source: Extract<T.RecoverySource<number>, { type: 'backup' }> = {
      type: 'backup',
      target: {
        type: 'cifs',
        hostname: 'host',
        path: 'backup',
        username: 'user',
        password: null,
      },
      password: 42,
      serverId: 'server',
    }
    const target: T.BackupTargetFS = source.target
    expect(target.type).toBe('cifs')
    source.target = {
      type: 'cifs',
      hostname: 'host',
      path: 'backup',
      username: 'user',
      // @ts-expect-error Only the top-level backup password is replaced.
      password: 42,
    }
    // @ts-expect-error Server identifiers remain strings.
    source.serverId = 42
    // @ts-expect-error Backup passwords use the supplied type.
    source.password = 'secret'
    // @ts-expect-error Disk targets require a logical device name.
    source.target = { type: 'disk' }
  })

  test('recovery discriminators and required fields remain strict', () => {
    const discriminator: T.RecoverySource<number> = {
      // @ts-expect-error Recovery sources require a known discriminator.
      type: 'restore',
      guid: 'guid',
    }
    // @ts-expect-error Backup sources require a server identifier.
    const missing: T.RecoverySource<number> = {
      type: 'backup',
      target: { type: 'disk', logicalname: '/dev/sda' },
      password: 42,
    }
    const migration: T.RecoverySource<number> = {
      type: 'migrate',
      guid: 'guid',
      // @ts-expect-error Migration sources do not acquire backup password fields.
      password: 42,
    }
    expect([discriminator.type, missing.type, migration.type]).toEqual([
      'restore',
      'backup',
      'migrate',
    ])
  })

  test('registry commitments accept arbitrary caller types', () => {
    const asset: T.RegistryAsset<{ digest: Uint8Array }> = {
      publishedAt: '2026-01-01T00:00:00Z',
      urls: ['https://example.com/asset'],
      commitment: { digest: new Uint8Array([1]) },
      signatures: { signer: 'signature' },
    }
    const signatures: T.RegistryAsset<string>['signatures'] = asset.signatures
    expect(asset.commitment.digest).toEqual(new Uint8Array([1]))
    expect(signatures.signer).toBe('signature')
    // @ts-expect-error Commitments use the supplied type.
    asset.commitment = 'digest'
  })

  test('generic positions preserve nullable and scalar types', () => {
    const source: T.RecoverySource<boolean | null> = {
      type: 'backup',
      target: { type: 'disk', logicalname: '/dev/sda' },
      password: null,
      serverId: 'server',
    }
    const asset: T.RegistryAsset<number | null> = {
      publishedAt: '2026-01-01T00:00:00Z',
      urls: [],
      commitment: 42,
      signatures: {},
    }
    expect(source.password).toBeNull()
    expect(asset.commitment).toBe(42)
    asset.commitment = null
    // @ts-expect-error Nullable numeric commitments do not accept strings.
    asset.commitment = 'digest'
  })

  test('registry metadata retains the generated owner types', () => {
    const asset: T.RegistryAsset<number> = {
      publishedAt: '2026-01-01T00:00:00Z',
      urls: [],
      commitment: 42,
      signatures: {},
    }
    expect(asset.commitment).toBe(42)
    // @ts-expect-error Publication timestamps remain strings.
    asset.publishedAt = new Date()
    // @ts-expect-error Asset URLs remain strings.
    asset.urls = [42]
    // @ts-expect-error Signatures remain strings.
    asset.signatures = { signer: 42 }
    // @ts-expect-error Registry metadata remains required.
    const missing: T.RegistryAsset<number> = { commitment: 42 }
    expect(missing.commitment).toBe(42)
  })
})
