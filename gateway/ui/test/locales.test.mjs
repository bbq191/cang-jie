// 语言包一致性（node --test gateway/ui/test/）：中英两份键集合相同、同一个键的 {占位符} 相同、页面脚本里字面量
// T('键') 引用的键都在语言包里、语言包里没有页面脚本不再引用的死键。缺键时 T() 会静默显示键名本身，
// 占位符对不上会把 "{name}" 原样漏给用户——这类问题人工对两份 JSON 很难看出来。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { read, scriptOrder } from './load.mjs';

const ui = join(dirname(fileURLToPath(import.meta.url)), '..');
const zh = JSON.parse(readFileSync(join(ui, 'locales/zh-CN.json'), 'utf8'));
const en = JSON.parse(readFileSync(join(ui, 'locales/en-US.json'), 'utf8'));
// 页面脚本 = ui.rs 按顺序拼起来的几个源文件（见 load.mjs）。
const src = scriptOrder().map(read).join('\n');

test('中英键集合一致', () => {
  assert.deepEqual(Object.keys(en).filter(k => !(k in zh)), [], 'en 多出的键');
  assert.deepEqual(Object.keys(zh).filter(k => !(k in en)), [], 'zh 多出的键');
});

test('同一个键两种语言的 {占位符} 相同', () => {
  const ph = s => [...s.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort().join(',');
  const bad = Object.keys(zh).filter(k => ph(zh[k]) !== ph(en[k] || ''));
  assert.deepEqual(bad, []);
});

test("页面脚本字面量 T('键') 都在语言包里", () => {
  // 键都带点（注释里举例用的 T('xxx') 不算）
  const used = [...src.matchAll(/\bT\(\s*'([^'$.]+\.[^'$]+)'\s*[,)]/g)].map(m => m[1]);
  assert.ok(used.length > 100, '没抓到 T() 调用，正则可能失效了');
  assert.deepEqual([...new Set(used.filter(k => !(k in zh)))], []);
});

test('语言包里没有死键（动态拼接的键按字面量前缀放行）', () => {
  // 源码里所有形如 'a.b.c' 的字面量（含常量表里只存键名的写法）+ 以 '.' 结尾、后面紧跟拼接的前缀
  const lits = new Set([...src.matchAll(/['"`]([a-zA-Z][\w-]*(?:\.[\w-]+)+)['"`]/g)].map(m => m[1]));
  const prefixes = [...src.matchAll(/['"`]([a-zA-Z][\w.-]*\.)['"`]\s*\+/g)].map(m => m[1]);
  const dead = Object.keys(zh).filter(k => !lits.has(k) && !prefixes.some(p => k.startsWith(p)));
  assert.deepEqual(dead, [], '这些键页面脚本已不再引用（删掉，或确认是动态拼接后补前缀）');
});
