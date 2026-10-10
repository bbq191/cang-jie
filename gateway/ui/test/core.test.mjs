// core.js 纯函数单测（node --test gateway/ui/test/）：整文件加载 core.js（见 load.mjs），不碰 DOM。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './load.mjs';

const get = load(['core.js']);

test('格式徽章：白名单格式照 EXT.book 显示，format:"other" 显示真实扩展名、不冒充 EPUB', () => {
  const f = get('bookFmtLabel');
  assert.equal(f({ name: 'a.epub', format: 'epub' }), 'EPUB');
  assert.equal(f({ name: 'a.pdf', format: 'pdf' }), 'PDF');
  assert.equal(f({ name: '旧漫画.cbz', format: 'other' }), 'CBZ');
  assert.equal(f({ name: 'README', format: 'other' }), 'stg.fmt.other', '没有扩展名时显示"其他"');
});

// 笔记页刷新闸门（refreshGate）：时间注入，逐条钉住改造前散落标志的行为。
const gateAt = () => { let t = 1000; const g = get('refreshGate')(() => t); const N = get('NOTE_GATE'); return { g, N, tick: ms => { t += ms; } }; };

test('refreshGate ① hold：挡 SSE 触发的一轮（force 消耗、list/checkImport 留到下一轮），到期或 force 放行', () => {
  const { g, N, tick } = gateAt();
  g.request(true, true, false);
  g.hold(N.HOLD_BUSY_MS, 'busy');
  assert.equal(g.held(), true);
  assert.equal(g.take(), null, '请求在跑：SSE 触发的刷新被挡');
  assert.equal(g.st.last, 'hold:busy', '记下是哪道闸、为什么');
  g.hold(N.LINGER_MS, 'linger');               // 结果出来：改成再挡 LINGER_MS
  tick(N.LINGER_MS - 1);
  assert.equal(g.take(), null);
  tick(1);
  assert.deepEqual({ ...g.take() }, { list: true, checkImport: true, force: false }, '到期后，之前被挡的那轮的 list/checkImport 还在');
  assert.deepEqual({ ...g.take() }, { list: false, checkImport: false, force: false }, '取走后清零');
  g.hold(N.LINGER_MS, 'linger');
  g.request(false, false, true);
  assert.deepEqual({ ...g.take() }, { list: false, checkImport: false, force: true }, 'force（用户自己换书）不受 hold 挡');
  g.hold(0);
  assert.equal(g.held(), false, 'hold(0) 立即解除');
  assert.equal(g.st.holdWhy, '');
});

test('refreshGate ② quiet：自己重取期间与之后 SELF_QUIET_MS 内的 entries 事件不理，别的事件照常', () => {
  const { g, N, tick } = gateAt();
  assert.equal(g.event('entries', false), 'refresh');
  g.quietBegin();
  tick(60000);
  assert.equal(g.event('entries', false), 'quiet', '重取还没结束：一直不理');
  assert.equal(g.event('transcribe', false), 'refresh', '只挡 entries');
  g.quietEnd();
  tick(N.SELF_QUIET_MS - 1);
  assert.equal(g.event('entries', false), 'quiet');
  tick(1);
  assert.equal(g.event('entries', false), 'refresh');
  assert.equal(g.event('notebooks', false), 'sync', 'notebooks 只刷同步状态');
});

test('refreshGate ③ editing：输入时任何事件都只记一笔，失焦后补一次且只补一次', () => {
  const { g } = gateAt();
  assert.equal(g.blurred(false), false, '没有被挡的事件：失焦不刷');
  assert.equal(g.event('notebooks', true), 'defer');
  assert.equal(g.event('entries', true), 'defer');
  assert.equal(g.blurred(true), false, '焦点还在本 tab 的输入框里：不补');
  assert.equal(g.blurred(false), true);
  assert.equal(g.blurred(false), false, '只补一次');
});
