// 母版库低空间判定（node --test gateway/ui/test/）：新 book-serve 给 lowSpace 布尔就照它；旧版没有这个字段时
// 退回前端按 300MB 判，保证新前端配旧后端不坏。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const src = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'app.js'), 'utf8');
const m = src.match(/^const stagingLowSpace=.*;$/m);
assert.ok(m, 'app.js 里找不到 `const stagingLowSpace=…;`');
const low = new Function(`${m[0]}; return stagingLowSpace;`)();
const MB = 1048576;

test('后端给了 lowSpace：以它为准，不管 freeBytes', () => {
  assert.equal(low({ lowSpace: true, freeBytes: 10 * 1024 * MB }), true);
  assert.equal(low({ lowSpace: false, freeBytes: 1 * MB }), false);
  assert.equal(low({ lowSpace: false }), false);
});

test('旧版后端没有 lowSpace：退回 300MB 前端判断', () => {
  assert.equal(low({ freeBytes: 299 * MB }), true);
  assert.equal(low({ freeBytes: 300 * MB }), false);
  assert.equal(low({}), false, '连 freeBytes 都没有就不告警');
  assert.equal(low({ freeBytes: null }), false);
});
