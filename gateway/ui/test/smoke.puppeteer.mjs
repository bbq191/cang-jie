// 前端冒烟（真浏览器，需要 puppeteer 或 playwright；**手动跑，不进 CI**——CI 只跑零依赖的 xss.test.mjs）。
// 用法：PUPPETEER_NODE_MODULES=<含 puppeteer 的 node_modules 目录> node gateway/ui/test/smoke.puppeteer.mjs
// （本机可用 mermaid-cli 自带的：/…/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/）；
// 或 PLAYWRIGHT_NODE_MODULES=<含 playwright 的 node_modules 目录>（如 gateway/tools/screenshot-walkthrough/node_modules）。
// 不起网关：拦截请求直接喂拼好的页面，并在页面里 mock 掉 fetch 与 EventSource，验证：
//   ① 文件名/错误文案里的 HTML 不会注入 DOM（无 <img>、onerror 不触发）；
//   ② 10 个 SSE 事件连发被 coalesce 成至多 2 次刷新；网关自身的批量/闸门事件只重取那两个状态，不全量刷新；③ 页面隐藏时不刷新、可见后补刷一次；④ 空闲无轮询。
import { createRequire } from 'node:module';
import fs from 'node:fs';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const PW = process.env.PLAYWRIGHT_NODE_MODULES;
const require = createRequire((PW || process.env.PUPPETEER_NODE_MODULES || process.cwd() + '/node_modules') + '/');
const UI = join(dirname(fileURLToPath(import.meta.url)), '..') + '/';
let html = fs.readFileSync(UI + 'index.html', 'utf8');
const exts = JSON.stringify({book:['epub','pdf'],native:['epub','pdf'],font:['ttf'],dict:['ifo'],image:['png']});
html = html.replace('__STYLE__', fs.readFileSync(UI + 'style.css','utf8')).replace('__SCRIPT__', fs.readFileSync(UI + 'app.js','utf8').replace('__EXTS__', exts));
const zh = fs.readFileSync(UI + 'locales/zh-CN.json','utf8');
const mock = `
window.__hits = {}; window.__es = []; window.__hidden = false; window.__xss = 0;
Object.defineProperty(document, 'hidden', {get: () => window.__hidden});
class ES { constructor(u){ this.u=u; this.closed=false; window.__es.push(this); setTimeout(() => this.onopen && !this.closed && this.onopen(), 10);} close(){ this.closed=true; } }
const __st = window.setTimeout.bind(window); window.setTimeout = (f, ms, ...a) => __st(f, ms === 60000 && window.__hiddenCloseMs ? window.__hiddenCloseMs : ms, ...a);
window.EventSource = ES;
const zh = ${JSON.stringify(zh)};
const evil = '<img src=x onerror=window.__xss=1>.epub';
const routes = {
  '/ui/locales/zh-CN.json': () => JSON.parse(zh),
  '/api/services': () => ({services: []}),
  '/api/manage': () => ({modules: []}),
  '/api/books/staging': () => ({ok:true, items:[{name: evil, format:'epub', bytes:1000, mtime:1, optimized:false, delivered:{optimize:{status:'failed', message:'"><img src=x onerror=window.__xss=1>'}}}], freeBytes: 9e9}),
  '/api/books/status': () => ({ok:true, xochitlFolders:[]}),
  '/api/koreader/status': () => ({ok:true, installed:false}),
  '/api/koreader/books': () => ({items:[]}),
  '/api/budget/status': () => ({pending:[], active:[]}),
  '/api/batch/status': () => ({running:false,total:0,done:0,queued:[],failed:[]}),
  '/api/foundation': () => ({}), '/api/enhance/status': () => ({}),
};
window.fetch = async (url, opt) => {
  const p = String(url).split('?')[0];
  window.__hits[p] = (window.__hits[p]||0) + 1;
  await new Promise(r => setTimeout(r, 30));
  const f = routes[p]; const body = f ? f() : {};
  return { status: 200, ok: true, json: async () => body, text: async () => JSON.stringify(body) };
};
`;
html = html.replace('<head>', '<head><script>' + mock + '</script>');
let browser, page;
if (PW) {
  browser = await require('playwright').chromium.launch();
  page = await browser.newPage();
  await page.route('**/*', r => r.request().url() === 'http://shelf.test/' ? r.fulfill({status:200, contentType:'text/html', body: html}) : r.abort());
} else {
  browser = await require('puppeteer').launch({ headless: true, args: ['--no-sandbox'] });
  page = await browser.newPage();
  await page.setRequestInterception(true);
  page.on('request', r => r.url() === 'http://shelf.test/' ? r.respond({status:200, contentType:'text/html', body: html}) : r.abort());
}
const errs = [];
page.on('pageerror', e => errs.push('pageerror: ' + e.message));
page.on('console', m => { if (m.type() === 'error') errs.push('console: ' + m.text()); });
await page.goto('http://shelf.test/', { waitUntil: 'load' });
await new Promise(r => setTimeout(r, 600));
const hits = (p = '/api/books/staging') => page.evaluate(p => window.__hits[p] || 0, p);
const out = {};
out.initialHits = await hits();
out.xss = await page.evaluate(() => window.__xss);
out.listText = await page.evaluate(() => (document.querySelector('#stglist')||{}).textContent || '');
out.imgInjected = await page.evaluate(() => document.querySelectorAll('#stglist img').length);
// 事件突发：10 个 book-serve 事件连发 → 合并
await page.evaluate(() => { for (let i = 0; i < 10; i++) window.__es[0].onmessage({data: JSON.stringify({area:'books', kind:'staging', svc:'books'})}); });
await new Promise(r => setTimeout(r, 500));
out.afterBurst = (await hits()) - out.initialHits;
// 网关自身的批量/闸门事件（不带 svc）：只重取批量/闸门状态，不全量刷新
const s0 = await hits(), b0 = await hits('/api/batch/status');
await page.evaluate(() => { for (let i = 0; i < 5; i++) window.__es[0].onmessage({data: JSON.stringify({area:'books', kind: i % 2 ? 'budget' : 'batch'})}); });
await new Promise(r => setTimeout(r, 500));
out.queueEventStagingHits = (await hits()) - s0;
out.queueEventBatchHits = (await hits('/api/batch/status')) - b0;
// 隐藏时不刷新
const h0 = await hits();
await page.evaluate(() => { window.__hidden = true; window.__es[0].onmessage({data: JSON.stringify({area:'books'})}); });
await new Promise(r => setTimeout(r, 300));
out.hiddenHits = (await hits()) - h0;
// 可见后补刷一次
await page.evaluate(() => { window.__hidden = false; document.dispatchEvent(new Event('visibilitychange')); });
await new Promise(r => setTimeout(r, 300));
out.afterVisible = (await hits()) - h0;
// 搜索框防抖：连敲字不立即整表重画，停手 150ms 后才生效
out.searchImmediate = await page.evaluate(() => { const q = document.querySelector('#stgq'); q.value = 'zzz-no-match'; q.dispatchEvent(new Event('input')); return document.querySelectorAll('#stglist .stg-row').length; });
await new Promise(r => setTimeout(r, 400));
out.searchAfter = await page.evaluate(() => { const q = document.querySelector('#stgq'); const n = document.querySelectorAll('#stglist .stg-row').length; q.value = ''; q.dispatchEvent(new Event('change')); return n; });
// 隐藏超时后断开 SSE、重新可见时重连并补刷一次（把 60s 缩成 50ms）；心跳参数 ka=60
out.esUrl = await page.evaluate(() => window.__es[0].u);
await page.evaluate(() => { window.__hiddenCloseMs = 50; window.__hidden = true; document.dispatchEvent(new Event('visibilitychange')); });
await new Promise(r => setTimeout(r, 300));
out.esClosedWhileHidden = await page.evaluate(() => window.__es[window.__es.length - 1].closed);
const h2 = await hits();
await page.evaluate(() => { window.__hidden = false; document.dispatchEvent(new Event('visibilitychange')); });
await new Promise(r => setTimeout(r, 400));
out.esCount = await page.evaluate(() => window.__es.length);
out.esReopenedOpen = await page.evaluate(() => !window.__es[window.__es.length - 1].closed);
out.reopenRefresh = (await hits()) - h2;
// 无轮询：等 4 秒无新请求
const h1 = await hits();
await new Promise(r => setTimeout(r, 4000));
out.idleHits = (await hits()) - h1;
await browser.close();
console.log(JSON.stringify({ ...out, errs }, null, 1));
assert.equal(out.xss, 0, '含 HTML 的文件名/错误文案不能执行脚本');
assert.equal(out.imgInjected, 0, '不能注入 <img>');
assert.ok(out.listText.includes('<img src=x'), '文件名应作为文本显示');
assert.ok(out.afterBurst >= 1 && out.afterBurst <= 2, `事件突发应合并，实际 ${out.afterBurst} 次`);
assert.equal(out.queueEventStagingHits, 0, '网关排队/进度事件不该全量重取母版库列表');
assert.ok(out.queueEventBatchHits >= 1 && out.queueEventBatchHits <= 2, `排队事件应重取批量状态且合并，实际 ${out.queueEventBatchHits} 次`);
assert.equal(out.hiddenHits, 0, '页面隐藏时不该刷新');
assert.equal(out.afterVisible, 1, '可见后应补刷一次');
assert.equal(out.searchImmediate, 1, '敲字后同一时刻不该立即重画（防抖）');
assert.equal(out.searchAfter, 0, '防抖到点后过滤应生效');
assert.ok(out.esUrl.includes('ka=60'), 'SSE 应带 ?ka=60 拉长心跳');
assert.equal(out.esClosedWhileHidden, true, '页面隐藏超时后应断开 SSE');
assert.equal(out.esCount, 2, '重新可见应重连（新建一条 EventSource）');
assert.equal(out.esReopenedOpen, true);
assert.equal(out.reopenRefresh, 1, '重连成功应补刷当前 tab 一次');
assert.equal(out.idleHits, 0, '空闲时不该有轮询');
assert.deepEqual(errs, [], '不该有 JS 报错');
console.log('smoke OK');
