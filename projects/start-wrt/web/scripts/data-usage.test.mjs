import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { stripTypeScriptTypes } from 'node:module'
import test from 'node:test'

const source = await readFile(
  new URL('../src/app/routes/devices/data-usage.ts', import.meta.url),
  'utf8',
)
const code = stripTypeScriptTypes(source, { mode: 'transform' })
const {
  BYTES_PER_KB,
  BYTES_PER_MB,
  BYTES_PER_GB,
  BYTES_PER_TB,
  formatDataUsageMb,
  computeChartYAxis,
  buildChartLines,
} = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
)

test('formatDataUsageMb returns null for missing or zero totals', () => {
  assert.equal(formatDataUsageMb(null), null)
  assert.equal(formatDataUsageMb(undefined), null)
  assert.equal(formatDataUsageMb(0), null)
})

test('formatDataUsageMb never displays non-zero usage as zero', () => {
  assert.equal(formatDataUsageMb(1), '< 0.1')
  assert.equal(formatDataUsageMb(32 * BYTES_PER_KB), '< 0.1')
  assert.equal(formatDataUsageMb(104_857), '< 0.1')
  assert.equal(formatDataUsageMb(104_858), '0.1')
  assert.equal(formatDataUsageMb(1, 'de-DE'), '< 0,1')
})

test('formatDataUsageMb formats MB with 1 decimal below 100 and integer with separators from 100 up', () => {
  assert.equal(formatDataUsageMb(Math.round(0.4 * BYTES_PER_MB)), '0.4')
  assert.equal(formatDataUsageMb(Math.round(5.6 * BYTES_PER_MB)), '5.6')
  assert.equal(formatDataUsageMb(12 * BYTES_PER_MB), '12.0')
  assert.equal(formatDataUsageMb(Math.round(40.3 * BYTES_PER_MB)), '40.3')
  assert.equal(formatDataUsageMb(Math.round(99.94 * BYTES_PER_MB)), '99.9')
  assert.equal(formatDataUsageMb(Math.round(99.96 * BYTES_PER_MB)), '100')
  assert.equal(formatDataUsageMb(100 * BYTES_PER_MB), '100')
  assert.equal(formatDataUsageMb(1771 * BYTES_PER_MB), '1,771')
  assert.equal(formatDataUsageMb(37692 * BYTES_PER_MB), '37,692')
  assert.equal(formatDataUsageMb(37692 * BYTES_PER_MB, 'de-DE'), '37.692')
})

test('computeChartYAxis scales down below 0.5 GB for quiet devices and matches binary units', () => {
  const zero = computeChartYAxis([{ download: 0, upload: 0 }])
  assert.equal(zero.maxBytes, 0)
  assert.deepEqual(zero.labels, ['0'])

  const oneByte = computeChartYAxis([{ download: 1, upload: 0 }])
  assert.equal(oneByte.maxBytes, 3)
  assert.deepEqual(oneByte.labels, ['0', '1 B', '2 B', '3 B'])

  const fourBytes = computeChartYAxis([{ download: 4, upload: 0 }])
  assert.equal(fourBytes.maxBytes, 6)
  assert.deepEqual(fourBytes.labels, ['0', '2 B', '4 B', '6 B'])

  const sevenBytes = computeChartYAxis([{ download: 7, upload: 0 }])
  assert.equal(sevenBytes.maxBytes, 9)
  assert.deepEqual(sevenBytes.labels, ['0', '3 B', '6 B', '9 B'])

  for (const axis of [oneByte, fourBytes, sevenBytes]) {
    assert.equal(Number.parseFloat(axis.labels.at(-1)), axis.maxBytes)
  }

  const subMb = computeChartYAxis([
    { download: Math.round(0.4 * BYTES_PER_MB), upload: 50 * BYTES_PER_KB },
  ])
  assert.equal(subMb.maxBytes, 450 * BYTES_PER_KB)
  assert.deepEqual(subMb.labels, ['0', '150 KB', '300 KB', '450 KB'])

  const twelveMb = computeChartYAxis([
    { download: 12 * BYTES_PER_MB, upload: 2 * BYTES_PER_MB },
  ])
  assert.equal(twelveMb.maxBytes, 12 * BYTES_PER_MB)
  assert.deepEqual(twelveMb.labels, ['0', '4 MB', '8 MB', '12 MB'])

  const fortyMb = computeChartYAxis([
    { download: 40 * BYTES_PER_MB, upload: 5 * BYTES_PER_MB },
  ])
  assert.equal(fortyMb.maxBytes, 45 * BYTES_PER_MB)
  assert.deepEqual(fortyMb.labels, ['0', '15 MB', '30 MB', '45 MB'])

  const promoteToGb = computeChartYAxis([
    { download: 950 * BYTES_PER_MB, upload: 100 * BYTES_PER_MB },
  ])
  assert.equal(promoteToGb.maxBytes, Math.round(1.2 * BYTES_PER_GB))
  assert.deepEqual(promoteToGb.labels, ['0', '0.4 GB', '0.8 GB', '1.2 GB'])

  const multiGb = computeChartYAxis([
    { download: 35.1 * BYTES_PER_GB, upload: 4 * BYTES_PER_GB },
  ])
  assert.equal(multiGb.maxBytes, 45 * BYTES_PER_GB)
  assert.deepEqual(multiGb.labels, ['0', '15 GB', '30 GB', '45 GB'])

  const multiTb = computeChartYAxis([
    { download: 1.5 * BYTES_PER_TB, upload: 0.2 * BYTES_PER_TB },
  ])
  assert.equal(multiTb.maxBytes, 1.5 * BYTES_PER_TB)
  assert.deepEqual(multiTb.labels, ['0', '0.5 TB', '1.0 TB', '1.5 TB'])
})

test('computeChartYAxis always produces ceiling >= peak data point', () => {
  const samples = [
    1,
    10,
    512,
    1023,
    1024,
    32 * BYTES_PER_KB,
    400 * BYTES_PER_KB,
    BYTES_PER_MB,
    1.1 * BYTES_PER_MB,
    40.3 * BYTES_PER_MB,
    900 * BYTES_PER_MB,
    950 * BYTES_PER_MB,
    1.7 * BYTES_PER_GB,
    35.1 * BYTES_PER_GB,
    2.7 * BYTES_PER_TB,
  ]
  for (const peak of samples) {
    const axis = computeChartYAxis([{ download: peak, upload: 0 }])
    assert.ok(
      axis.maxBytes >= peak,
      `expected maxBytes (${axis.maxBytes}) >= peak (${peak})`,
    )
    assert.ok(
      peak / axis.maxBytes >= 0.3,
      `expected peak (${peak}) to occupy a visible share of maxBytes (${axis.maxBytes})`,
    )
  }
})

test('buildChartLines aligns plotted coordinates to yAxis ceiling', () => {
  assert.deepEqual(buildChartLines([], 100, 400, 160), [])

  const single = buildChartLines(
    [{ download: 15 * BYTES_PER_MB, upload: 30 * BYTES_PER_MB }],
    45 * BYTES_PER_MB,
    400,
    150,
  )
  assert.deepEqual(single, [
    [
      [0, 50],
      [400, 50],
    ],
    [
      [0, 100],
      [400, 100],
    ],
  ])

  const points = [
    { download: 0, upload: 0 },
    { download: 15 * BYTES_PER_MB, upload: 7.5 * BYTES_PER_MB },
    { download: 40 * BYTES_PER_MB, upload: 15 * BYTES_PER_MB },
  ]
  const axis = computeChartYAxis(points)
  const [dl, ul] = buildChartLines(points, axis.maxBytes, 400, 160)
  assert.equal(dl[0][1], 0)
  assert.ok(Math.abs(dl[1][1] - 160 / 3) < 1e-9)
  assert.ok(Math.abs(dl[2][1] - (40 / 45) * 160) < 1e-9)
  assert.ok(dl.every(([, y]) => y <= 160))
  assert.ok(Math.abs(ul[2][1] - 160 / 3) < 1e-9)
})
