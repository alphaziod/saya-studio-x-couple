import test from 'node:test';
import assert from 'node:assert/strict';
import { extensionFor, extensionMismatch, parseLoras } from '../src/lib/ui-helpers.ts';

test('extension de destination suit le format', () => {
  assert.equal(extensionFor('/a/b - retouche.png', 'jpeg'), '/a/b - retouche.jpg');
  assert.equal(extensionFor('/a/b.JPG', 'png'), '/a/b.png');
  assert.equal(extensionFor('/a/b.jpeg', 'jpeg'), '/a/b.jpeg');
  assert.equal(extensionFor('/a/b.png', 'png'), '/a/b.png');
  assert.equal(extensionFor('/a/b.webp', 'jpeg'), '/a/b.webp');
  assert.equal(extensionFor('/a.png.d/b', 'jpeg'), '/a.png.d/b');
  assert.equal(extensionMismatch('/a/b.png', 'jpeg'), true);
  assert.equal(extensionMismatch('/a/b.jpg', 'png'), true);
  assert.equal(extensionMismatch('/a/b.jpg', 'jpeg'), false);
  assert.equal(extensionMismatch('/a/b.png', 'png'), false);
});

test('LoRA nom:poids vers la forme Rust', () => {
  assert.deepEqual(parseLoras(''), []);
  assert.deepEqual(parseLoras('detail_tweaker:0.6\r\n\n dossier/x.safetensors : 1 '), [
    { name: 'detail_tweaker', weightModel: 0.6, weightClip: 0.6 },
    { name: 'dossier/x.safetensors', weightModel: 1, weightClip: 1 }
  ]);
  assert.throws(() => parseLoras('sans_poids'), /ligne 1/);
  assert.throws(() => parseLoras('ok:1\nx:abc'), /ligne 2/);
  assert.throws(() => parseLoras(':0.5'), /ligne 1/);
});

// Regression: the casting pickers' grid has a fixed height. Without content-sized rows it shared
// that height between rows and squashed every tile (names 2 px tall, action list looking empty).
test('casting picker rows keep their content height', async () => {
  const { readFile } = await import('node:fs/promises');
  const source = await readFile(new URL('../src/components/Casting.svelte', import.meta.url), 'utf8');
  const escape = text => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const rule = selector => source.match(new RegExp(`${escape(selector)}\\{([^}]*)\\}`))?.[1] ?? '';
  assert.match(rule('.cibler-grille'), /grid-auto-rows:max-content/);
  assert.match(rule('.cibler-grille'), /align-content:start/);
  assert.doesNotMatch(rule('.cibler-tuile'), /min-height:0/);
  assert.match(rule('.cibler-tuile>*'), /flex:none/);
});
