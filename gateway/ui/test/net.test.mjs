// 网络层异常兜底回归（node --test gateway/ui/test/）：设备休眠 / WiFi 断开时 fetch 抛 TypeError，
// `j()` 必须转成 {ok:false,message} 而不是把异常抛给调用方（开关永久灰着、按钮没反应）。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './load.mjs';

// 语言包没加载（I18N 空）时 T() 原样返回键名，断言直接比键名。
const make = (fetch, location = {}) => load(['core.js'], { fetch, location })('j');

test('fetch 抛网络异常 → j 返回 ok:false + 网络错误文案', async () => {
  const j = make(async () => { throw new TypeError('Failed to fetch'); });
  assert.deepEqual({ ...(await j('/api/x')) }, { ok: false, message: 'common.networkError' });
});

test('非 JSON 的 502 → 人话 + 状态码', async () => {
  const j = make(async () => ({ status: 502, ok: false, json: async () => { throw new SyntaxError('x'); } }));
  const r = await j('/api/x');
  assert.equal(r.ok, false);
  assert.equal(r.message, 'common.httpErr');
});

/** 够 dom.js 里 toast/el 用的最小假 DOM：只记下 toast 文字。 */
function fakeDom() {
  const texts = [];
  const node = () => ({ style: {}, classList: { add() {}, remove() {} }, setAttribute() {}, appendChild() {}, remove() {} });
  const document = { body: node(), getElementById: () => null, createElement: node, createTextNode: t => { texts.push(t); return node(); } };
  return { texts, globals: { document, requestAnimationFrame: () => 0, setTimeout: () => 0 } };
}

test('guardClick 在 fn 抛异常时解禁按钮并提示', async () => {
  const dom = fakeDom();
  const guardClick = load(['core.js', 'dom.js'], dom.globals)('guardClick');
  const el = { disabled: false };
  guardClick(el, async () => { throw new Error('boom'); });
  await el.onclick();
  assert.equal(el.disabled, false);
  assert.deepEqual(dom.texts, ['common.failed']);
});

test('401 → 跳登录页（带回当前页）、403 → 跳改密页，j 都返回 ok:false', async () => {
  for (const [status, href, message] of [[401, '/login?next=%2Fx', 'common.needLogin'], [403, '/password', 'common.needChangePassword']]) {
    const loc = { pathname: '/x' };
    const j = make(async () => ({ status, ok: false, json: async () => ({}) }), loc);
    assert.deepEqual({ ...(await j('/api/x')) }, { ok: false, message });
    assert.equal(loc.href, href);
  }
});
