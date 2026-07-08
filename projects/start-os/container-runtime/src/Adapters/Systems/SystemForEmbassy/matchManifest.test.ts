import { matchManifest } from './matchManifest'
import giteaManifest from './__fixtures__/giteaManifest'
import synapseManifest from './__fixtures__/synapseManifest'

describe('matchManifest', () => {
  test('gittea', () => {
    matchManifest.parse(giteaManifest)
  })
  test('synapse', () => {
    matchManifest.parse(synapseManifest)
  })
})
