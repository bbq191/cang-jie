// 页面脚本拆分后的结构约束（node --test gateway/ui/test/）。2026-10-10 起 app.js 拆成几个源文件、编译期拼回一个
// <script>（顺序见 src/ui.rs 的 APP_JS）。这里兜住拆分引入的两类新风险：
// ① 拼接产物整体能解析（含"两个文件定义了同名顶层 const/let"——同一作用域重复声明是语法错误）。CI 的
//    `node --check gateway/ui/app.js` 只查最后那个文件，其余文件的语法靠这一条；
// ② 除最后的 app.js（启动代码）外，各文件加载时不碰 DOM/location/存储：在一个没有 document/window/location 的
//    上下文里依次加载不应抛错——否则测试没法整文件加载它们，也说明顶层混进了副作用。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { concatenated, load, scriptOrder } from './load.mjs';

test('拼接后的整段脚本可解析（无重复顶层声明）', () => {
  assert.doesNotThrow(() => new vm.Script(concatenated(), { filename: 'page.js' }));
});

test('启动代码以外的源文件加载时没有副作用', () => {
  const files = scriptOrder();
  assert.equal(files.at(-1), 'app.js', '启动代码必须拼在最后');
  assert.doesNotThrow(() => load(files.slice(0, -1)));
});
