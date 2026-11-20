import assert from 'node:assert/strict'
import test from 'node:test'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(new Map())
const { toUpsView } =
  await import('src/app/routes/portal/routes/system/routes/nut/status')

test('wall power requires an explicit online status', () => {
  for (const [status, label] of [
    ['OL', 'On wall power'],
    ['OL CHRG', 'On wall power'],
    ['OB', 'On battery'],
    ['OB LB', 'Low battery'],
    ['FSD', 'Shutting down'],
    ['OFF', 'Unknown'],
    ['WAIT', 'Unknown'],
    ['', 'Unknown'],
  ]) {
    assert.equal(toUpsView({ 'ups.status': status }).power.label, label)
  }
})
