// 前端转义回归（node --test gateway/ui/test/）。没有 DOM 环境（不引 jsdom，CI 保持零 npm 依赖），
// 所以分两层：① `esc()` 本身的行为；② 对 ui/*.js 源码做静态检查——凡是往 innerHTML / insertAdjacentHTML /
// `{html:…}` 模板里插外部数据（文件名、书名、章节标题、划线/转写文本、AI 回答、服务端错误文案…）的
// `${…}`，必须包在 `esc(…)` 里。新加渲染代码漏了转义，这里会红。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load, read, scriptOrder } from './load.mjs';

// esc 直接取 core.js 里的实现（整文件加载，不另写一份跟实现漂移）。
const esc = load(['core.js'])('esc');

test('esc 转义 5 个 HTML 特殊字符，空值给空串', () => {
  assert.equal(esc('<img src=x onerror=alert(1)>.epub'), '&lt;img src=x onerror=alert(1)&gt;.epub');
  assert.equal(esc(`a"b'c&d`), 'a&quot;b&#39;c&amp;d');
  assert.equal(esc(null), '');
  assert.equal(esc(undefined), '');
  assert.equal(esc(0), '0');
  assert.equal(esc('</textarea><script>'), '&lt;/textarea&gt;&lt;script&gt;');
});

test('esc 后放进属性/文本位置不会再出现可解析的标签或引号', () => {
  const evil = '"><svg onload=alert(1)>';
  const out = esc(evil);
  assert.ok(!/[<>"]/.test(out), out);
});

// 外部数据字段名：出现在 `${…}` 里就必须过 esc。
const EXTERNAL = /\.(name|title|message|text|label|subhead|chapter_title|question|brief|lastError|version|cn|keyMasked|uuid)\b/;
// 明知安全的写法：编译期常量/服务名表（TABS 的 it.name/icon/label、模块表 m.service），以及 encodeURIComponent 拼 URL。
const SAFE_LINE = [/id="other-\$\{it\.name\}"/, /\$\{it\.icon\} \$\{it\.label\}/, /encodeURIComponent\(/, /cropUrl\(/, /\$\{o\.title/]; // o.title = assetTab 的编译期常量标题

test('ui/*.js：往 HTML 里插外部数据的 ${…} 全部经过 esc()', () => {
  const bad = [];
  for (const f of scriptOrder()) read(f).split('\n').forEach((line, i) => {
    const t = line.trim();
    if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('*')) return;
    if (!/innerHTML|insertAdjacentHTML|html:|<span|<div|<li|<td|<option|<b>|<img/.test(line)) return;
    if (SAFE_LINE.some(r => r.test(line))) return;
    for (const mm of line.matchAll(/\$\{((?:[^{}]|\{[^{}]*\})*)\}/g)) {
      // 去掉 T(...) 调用（语言包文案是编译期常量；vars 里的外部数据要求各处自己 esc，靠下面对 vars 的检查）
      const expr = mm[1].replace(/T\((?:[^()]|\([^()]*\))*\)/g, 'T()');
      if (EXTERNAL.test(expr) && !/esc\(/.test(expr)) bad.push(`${f}:${i + 1}: \${${expr.slice(0, 90)}}`);
    }
  });
  assert.deepEqual(bad, [], `这些插值疑似漏了 esc():\n${bad.join('\n')}`);
});
