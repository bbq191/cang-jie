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
