// 网络层异常兜底回归（node --test gateway/ui/test/）：设备休眠 / WiFi 断开时 fetch 抛 TypeError，
// `j()` 必须转成 {ok:false,message} 而不是把异常抛给调用方（开关永久灰着、按钮没反应）。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const src = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'app.js'), 'utf8');
// 从源码原样取出 j 的定义（两行：函数头那行 + 后面两行续行，到 `return d}` 为止）。
const m = src.match(/^async function j\(url,opt\)\{[\s\S]*?return d\}$/m);
assert.ok(m, 'app.js 里找不到 `async function j(url,opt){…return d}`');
// j 依赖 authRedirect（401/403 去向，上传器 XHR 也用它），一并原样取出。
const ar = src.match(/^const authRedirect=.*;$/m);
assert.ok(ar, 'app.js 里找不到 `const authRedirect=…;`');
const make = (fetch, location = {}) => new Function('fetch', 'T', 'location', `${ar[0]}\n${m[0]}; return j;`)(fetch, k => k, location);

test('fetch 抛网络异常 → j 返回 ok:false + 网络错误文案', async () => {
  const j = make(async () => { throw new TypeError('Failed to fetch'); });
  assert.deepEqual(await j('/api/x'), { ok: false, message: 'common.networkError' });
});

test('非 JSON 的 502 → 人话 + 状态码', async () => {
  const j = make(async () => ({ status: 502, ok: false, json: async () => { throw new SyntaxError('x'); } }));
  const r = await j('/api/x');
  assert.equal(r.ok, false);
  assert.equal(r.message, 'common.httpErr');
});

test('guardClick 在 fn 抛异常时解禁按钮并提示', async () => {
  const g = src.match(/^const guardClick=.*;$/m);
  assert.ok(g);
  const toasts = [];
  const guardClick = new Function('toast', 'T', `${g[0]}; return guardClick;`)(msg => toasts.push(msg), k => k);
  const el = { disabled: false };
  guardClick(el, async () => { throw new Error('boom'); });
  await el.onclick();
  assert.equal(el.disabled, false);
  assert.deepEqual(toasts, ['common.failed']);
});

test('401 → 跳登录页（带回当前页）、403 → 跳改密页，j 都返回 ok:false', async () => {
  for (const [status, href, message] of [[401, '/login?next=%2Fx', 'common.needLogin'], [403, '/password', 'common.needChangePassword']]) {
    const loc = { pathname: '/x' };
    const j = make(async () => ({ status, ok: false, json: async () => ({}) }), loc);
    assert.deepEqual(await j('/api/x'), { ok: false, message });
    assert.equal(loc.href, href);
  }
});
