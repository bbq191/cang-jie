// 前端冒烟（真浏览器，需要 puppeteer；**手动跑，不进 CI**——CI 只跑零依赖的 xss.test.mjs）。
// 用法：PUPPETEER_NODE_MODULES=<含 puppeteer 的 node_modules 目录> node gateway/ui/test/smoke.puppeteer.mjs
// （本机可用 mermaid-cli 自带的：/…/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/）。
// 不起网关：拦截请求直接喂拼好的页面，并在页面里 mock 掉 fetch 与 EventSource，验证：
//   ① 文件名/错误文案里的 HTML 不会注入 DOM（无 <img>、onerror 不触发）；
//   ② 10 个 SSE 事件连发被 coalesce 成至多 2 次刷新；③ 页面隐藏时不刷新、可见后补刷一次；④ 空闲无轮询。
import { createRequire } from 'node:module';
import fs from 'node:fs';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const require = createRequire((process.env.PUPPETEER_NODE_MODULES || process.cwd() + '/node_modules') + '/');
const puppeteer = require('puppeteer');
const UI = join(dirname(fileURLToPath(import.meta.url)), '..') + '/';
let html = fs.readFileSync(UI + 'index.html', 'utf8');
const exts = JSON.stringify({book:['epub','pdf'],native:['epub','pdf'],font:['ttf'],dict:['ifo'],image:['png']});
html = html.replace('__STYLE__', fs.readFileSync(UI + 'style.css','utf8')).replace('__SCRIPT__', fs.readFileSync(UI + 'app.js','utf8').replace('__EXTS__', exts));
const zh = fs.readFileSync(UI + 'locales/zh-CN.json','utf8');
const mock = `
window.__hits = {}; window.__es = []; window.__hidden = false; window.__xss = 0;
Object.defineProperty(document, 'hidden', {get: () => window.__hidden});
class ES { constructor(u){ this.u=u; window.__es.push(this);} close(){} }
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
const browser = await puppeteer.launch({ headless: true, args: ['--no-sandbox'] });
const page = await browser.newPage();
const errs = [];
page.on('pageerror', e => errs.push('pageerror: ' + e.message));
page.on('console', m => { if (m.type() === 'error') errs.push('console: ' + m.text()); });
html = html.replace('<head>', '<head><script>' + mock + '</script>');
await page.setRequestInterception(true);
page.on('request', r => r.url() === 'http://shelf.test/' ? r.respond({status:200, contentType:'text/html', body: html}) : r.abort());
await page.goto('http://shelf.test/', { waitUntil: 'load' });
await new Promise(r => setTimeout(r, 600));
const hits = () => page.evaluate(() => window.__hits['/api/books/staging'] || 0);
const out = {};
out.initialHits = await hits();
out.xss = await page.evaluate(() => window.__xss);
out.listText = await page.evaluate(() => (document.querySelector('#stglist')||{}).textContent || '');
out.imgInjected = await page.evaluate(() => document.querySelectorAll('#stglist img').length);
// 事件突发：10 个 books 事件连发 → 合并
const es = 'window.__es[0]';
await page.evaluate(() => { for (let i = 0; i < 10; i++) window.__es[0].onmessage({data: JSON.stringify({area:'books', kind:'batch'})}); });
await new Promise(r => setTimeout(r, 500));
out.afterBurst = (await hits()) - out.initialHits;
// 隐藏时不刷新
const h0 = await hits();
await page.evaluate(() => { window.__hidden = true; window.__es[0].onmessage({data: JSON.stringify({area:'books'})}); });
await new Promise(r => setTimeout(r, 300));
out.hiddenHits = (await hits()) - h0;
// 可见后补刷一次
await page.evaluate(() => { window.__hidden = false; document.dispatchEvent(new Event('visibilitychange')); });
await new Promise(r => setTimeout(r, 300));
out.afterVisible = (await hits()) - h0;
// 无轮询：等 4 秒无新请求
const h1 = await hits();
await new Promise(r => setTimeout(r, 4000));
out.idleHits = (await hits()) - h1;
await browser.close();
console.log(JSON.stringify({ ...out, errs }, null, 1));
assert.equal(out.xss, 0, '含 HTML 的文件名/错误文案不能执行脚本');
assert.equal(out.imgInjected, 0, '不能注入 <img>');
assert.ok(out.listText.includes('<img src=x'), '文件名应作为文本显示');
assert.ok(out.afterBurst <= 2, `事件突发应合并，实际 ${out.afterBurst} 次`);
assert.equal(out.hiddenHits, 0, '页面隐藏时不该刷新');
assert.equal(out.afterVisible, 1, '可见后应补刷一次');
assert.equal(out.idleHits, 0, '空闲时不该有轮询');
assert.deepEqual(errs, [], '不该有 JS 报错');
console.log('smoke OK');
