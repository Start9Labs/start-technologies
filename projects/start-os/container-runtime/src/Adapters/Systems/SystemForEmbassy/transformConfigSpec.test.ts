import {
  matchOldConfigSpec,
  matchOldValueSpecList,
  transformConfigSpec,
} from './transformConfigSpec'
import fixtureEmbassyPagesConfig from './__fixtures__/embassyPagesConfig'
import fixtureRTLConfig from './__fixtures__/rtlConfig'
import searNXG from './__fixtures__/searNXG'
import bitcoind from './__fixtures__/bitcoind'
import nostr from './__fixtures__/nostr'
import nostrConfig2 from './__fixtures__/nostrConfig2'

describe('transformConfigSpec', () => {
  test('matchOldConfigSpec(embassyPages.homepage.variants[web-page])', () => {
    matchOldConfigSpec.parse(
      fixtureEmbassyPagesConfig.homepage.variants['web-page'],
    )
  })
  test('matchOldConfigSpec(embassyPages)', () => {
    matchOldConfigSpec.parse(fixtureEmbassyPagesConfig)
  })
  test('transformConfigSpec(embassyPages)', () => {
    const spec = matchOldConfigSpec.parse(fixtureEmbassyPagesConfig)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })

  test('matchOldConfigSpec(RTL.nodes)', () => {
    matchOldValueSpecList.parse(fixtureRTLConfig.nodes)
  })
  test('matchOldConfigSpec(RTL)', () => {
    matchOldConfigSpec.parse(fixtureRTLConfig)
  })
  test('transformConfigSpec(RTL)', () => {
    const spec = matchOldConfigSpec.parse(fixtureRTLConfig)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })

  test('transformConfigSpec(searNXG)', () => {
    const spec = matchOldConfigSpec.parse(searNXG)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })
  test('transformConfigSpec(bitcoind)', () => {
    const spec = matchOldConfigSpec.parse(bitcoind)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })
  test('transformConfigSpec(nostr)', () => {
    const spec = matchOldConfigSpec.parse(nostr)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })
  test('transformConfigSpec(nostr2)', () => {
    const spec = matchOldConfigSpec.parse(nostrConfig2)
    expect(transformConfigSpec(spec)).toMatchSnapshot()
  })
})
