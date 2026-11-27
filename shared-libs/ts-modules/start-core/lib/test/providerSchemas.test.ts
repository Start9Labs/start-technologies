import {
  PreviewProviderChangeParams,
  SelectProviderParams,
} from '../osBindings/rpc/start-os.schemas'
import { PackageMetadata } from '../osBindings/types.schemas'

const selection = { id: 'consumer', dependencyId: 'bitcoin' }

test('omitted provider intent decodes to an unlocked clear request', () => {
  expect(SelectProviderParams.parse(selection)).toEqual({
    ...selection,
    providerId: null,
    locked: false,
  })
})

test('an explicit provider lock survives generated decoding', () => {
  const request = { ...selection, providerId: 'alternate', locked: true }
  expect(SelectProviderParams.parse(request)).toEqual(request)
})

test('provider previews fill absent versions and compatibility claims', () => {
  expect(PreviewProviderChangeParams.parse({ id: 'alternate' })).toEqual({
    id: 'alternate',
    version: null,
    satisfies: [],
    satisfiesPackages: {},
  })
})

test('metadata without compatibility claims decodes empty collections', () => {
  const claims = PackageMetadata.pick({
    satisfies: true,
    satisfiesPackages: true,
  })
  expect(claims.parse({})).toEqual({ satisfies: [], satisfiesPackages: {} })
  const explicit = {
    satisfies: ['1.0.0:0'],
    satisfiesPackages: { bitcoin: ['26.0.0:0'] },
  }
  expect(claims.parse(explicit)).toEqual(explicit)
})
