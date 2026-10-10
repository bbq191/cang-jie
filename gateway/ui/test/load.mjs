// 前端测试的公共加载器（2026-10-10 拆分源文件后）：脚本拼接顺序以 `src/ui.rs` 的 `APP_JS` 为准（网关编译期就是按它
// 拼成页面里唯一的 <script>）；测试**整文件**加载无副作用的源文件，不再用正则从源码里抠单个函数。
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import vm from 'node:vm';

export const UI = join(dirname(fileURLToPath(import.meta.url)), '..');

/** `src/ui.rs` 里 `APP_JS` 的 `include_str!("../ui/<名>.js")` 顺序。 */
export function scriptOrder() {
  const rs = readFileSync(join(UI, '..', 'src', 'ui.rs'), 'utf8');
  const block = rs.slice(rs.indexOf('const APP_JS'), rs.indexOf(');', rs.indexOf('const APP_JS')));
  const files = [...block.matchAll(/include_str!\("\.\.\/ui\/([\w-]+\.js)"\)/g)].map(m => m[1]);
  if (files.length < 6 || files[files.length - 1] !== 'app.js') throw new Error('ui.rs 的 APP_JS 拼接表解析失败：' + files);
  return files;
}

export const read = f => readFileSync(join(UI, f), 'utf8');

/** 格式白名单占位换成与网关注入同形的值（`ui.rs::page`）。 */
export const EXTS = JSON.stringify({ book: ['epub', 'pdf'], font: ['ttf', 'otf'], image: ['jpg', 'png'] });

/** 拼接后的整段脚本（与网关页面里的 <script> 同一份内容，白名单占位已替换）。 */
export const concatenated = () => scriptOrder().map(read).join('').replace('__EXTS__', EXTS);

/**
 * 在一个新的 vm 上下文里依次加载 `files`（如 ['core.js']），`globals` 是注入的宿主对象（fetch/location/document…）。
 * 返回 `get(名字)`：取脚本顶层定义（const/let 不挂在全局对象上，只能在同一上下文里按名字求值）。
 */
export function load(files, globals = {}) {
  const ctx = vm.createContext({ console, setTimeout, clearTimeout, Promise, ...globals });
  for (const f of files) vm.runInContext(read(f).replace('__EXTS__', EXTS), ctx, { filename: f });
  return name => vm.runInContext(name, ctx);
}
