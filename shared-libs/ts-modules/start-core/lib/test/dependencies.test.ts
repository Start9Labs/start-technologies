import { checkDependencies } from '../dependencies/dependencies'
import { Effects } from '../Effects'
import { CheckDependenciesResult, DependencyRequirement } from '../types'

const QUANTUM = '#quantum:1.5.2:0'

const depsFor = (
  versionRange: string,
  result: Partial<CheckDependenciesResult>,
) => {
  const requirement: DependencyRequirement = {
    kind: 'exists',
    id: 'filebrowser',
    versionRange,
  }
  const effects = {
    getDependencies: async () => [requirement],
    checkDependencies: async () => [
      {
        packageId: 'filebrowser',
        title: 'File Browser',
        installedVersion: null,
        satisfies: [],
        isRunning: true,
        tasks: {},
        healthChecks: {},
        ...result,
      },
    ],
  } as unknown as Effects
  return checkDependencies(effects)
}

describe('checkDependencies version checks', () => {
  test.each([
    ['an unflavored version in range', '^2.62.2:1', '2.63.23:0', [], true],
    ['a flavored version alone', '^2.62.2:1', QUANTUM, [], false],
    [
      'a flavored version standing in for one in range',
      '^2.62.2:1',
      QUANTUM,
      ['2.60.0:0', '2.63.23:0'],
      true,
    ],
    [
      'an unflavored version standing in for one in range',
      '^2.62.2:1',
      '2.60.0:0',
      ['2.63.23:0'],
      true,
    ],
    [
      'a flavored version standing in for one out of range',
      '^2.62.2:1',
      QUANTUM,
      ['2.60.0:0'],
      false,
    ],
    [
      'a flavored version against a flavored range',
      '>=#quantum:1.0.0:0',
      QUANTUM,
      [],
      true,
    ],
    [
      'an unflavored version against a flavored range',
      '>=#quantum:1.0.0:0',
      '2.63.23:0',
      [],
      false,
    ],
    [
      'a version excluded by the range, standing in for one that is not',
      '>=2.62.2:1 && !=2.63.23:0',
      '2.63.23:0',
      ['2.62.2:1'],
      false,
    ],
  ])('%s', async (_name, range, installedVersion, satisfies, expected) => {
    const deps = await depsFor(range, { installedVersion, satisfies })
    expect(deps.installedVersionSatisfied('filebrowser')).toBe(expected)
    expect(deps.satisfied()).toBe(expected)
    if (expected) {
      expect(() => deps.throwIfNotSatisfied('filebrowser')).not.toThrow()
    } else {
      expect(() => deps.throwIfNotSatisfied('filebrowser')).toThrow(
        'does not match expected version range',
      )
    }
  })

  test('the per-package and whole-set boolean checks are both callable', async () => {
    const deps = await depsFor('^2.62.2:1', {
      installedVersion: '2.63.23:0',
      satisfies: [],
    })
    expect(deps.satisfied('filebrowser')).toBe(true)
    expect(deps.satisfied()).toBe(true)
    expect(deps.healthCheckSatisfied('filebrowser')).toBe(true)
  })

  test('an absent dependency is answered before the range is parsed', async () => {
    const deps = await depsFor('not a range', {
      installedVersion: null,
      satisfies: [],
    })
    expect(deps.installedVersionSatisfied('filebrowser')).toBe(false)
  })

  test('an absent dependency satisfies nothing, whatever it claims', async () => {
    const deps = await depsFor('^2.62.2:1', {
      installedVersion: null,
      satisfies: ['2.63.23:0'],
    })
    expect(deps.installedVersionSatisfied('filebrowser')).toBe(false)
    expect(() => deps.throwIfNotSatisfied('filebrowser')).toThrow(
      'is not installed',
    )
  })
})
