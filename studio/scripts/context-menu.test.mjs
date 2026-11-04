import test from 'node:test';
import assert from 'node:assert/strict';
import { keepsNativeMenu, menuPosition } from '../src/lib/context-menu.ts';

const field = { closest: selector => (selector.includes('input') ? {} : null) };
const plain = { closest: () => null };

test('the WebView menu stays only in editable fields and on a selection', () => {
  assert.equal(keepsNativeMenu(field, ''), true);
  assert.equal(keepsNativeMenu(plain, 'copied text'), true);
  assert.equal(keepsNativeMenu(plain, '   '), false);
  assert.equal(keepsNativeMenu(plain, ''), false);
  assert.equal(keepsNativeMenu(null, ''), false);
});

test('a menu never leaves the window', () => {
  const viewport = { width: 1000, height: 800 };
  assert.deepEqual(menuPosition(100, 100, 240, 200, viewport), { x: 100, y: 100 });
  assert.deepEqual(menuPosition(990, 790, 240, 200, viewport), { x: 756, y: 596 });
  assert.deepEqual(menuPosition(-50, -50, 240, 200, viewport), { x: 4, y: 4 });
});
