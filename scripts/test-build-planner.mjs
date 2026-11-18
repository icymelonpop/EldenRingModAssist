import assert from 'node:assert/strict'
import { compareOrigins, origins, runesBetween } from '../src/buildPlanner.ts'

for (const origin of origins) {
  assert.equal(origin.stats.length, 8)
  assert.equal(origin.stats.reduce((a, b) => a + b, 0) - 79, origin.level)
}
assert.equal(runesBetween(1, 2), 673)
assert.equal(runesBetween(2, 3), 689)
assert.equal(runesBetween(1, 713), 1692566842)
assert.equal(runesBetween(150, 150), 0)

const strength = compareOrigins([40, 12, 25, 35, 15, 9, 9, 7], false)
assert.deepEqual([strength[0].origin.name, strength[0].level, strength[0].runes], ['방랑기사', 73, 773639])
assert.equal(strength.length, 10)
const all = compareOrigins(Array(8).fill(99), true)
assert.equal(all.length, 12)
assert.ok(all.every(row => row.level === 713))
const extra = origins.find(origin => origin.name === '중장기사')
assert.equal(compareOrigins(extra.stats, true).find(row => row.origin === extra).points, 0)
console.log('태생 12종과 룬 계산 검증 통과')
