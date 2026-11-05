import test from 'node:test';
import assert from 'node:assert/strict';
import { isReadOnlyAction, parseArgs, parseJsonValue, formatJson, pushHistory, parentDirectory } from '../src/lib/dev-mode.ts';

test('actions de lecture directes, tout le reste demande confirmation', () => {
  for (const a of ['preferences', 'gallery', 'data_paths', ' connection ']) assert.equal(isReadOnlyAction(a), true, a);
  // saya_status syncs the run and may submit its next pass: it asks for confirmation.
  for (const a of ['set_preference', 'gallery_cleanup', 'trash_assets', 'open_directory', 'saya_status', 'inconnue', '', 'preferences_x']) assert.equal(isReadOnlyAction(a), false, a);
});

test('arguments : vide, objet, erreurs', () => {
  assert.deepEqual(parseArgs('  '), { ok: true, value: {} });
  assert.deepEqual(parseArgs('{"a":1}'), { ok: true, value: { a: 1 } });
  assert.equal(parseArgs('{oops').ok, false);
  assert.equal(parseArgs('[1]').error, 'not-object');
  assert.equal(parseArgs('3').error, 'not-object');
  assert.equal(parseArgs('null').error, 'not-object');
  assert.equal(parseArgs('{"action":"x"}').error, 'reserved-key');
});

test('valeur de préférence : tout JSON valide', () => {
  assert.deepEqual(parseJsonValue('true'), { ok: true, value: true });
  assert.deepEqual(parseJsonValue('"a"'), { ok: true, value: 'a' });
  assert.deepEqual(parseJsonValue('null'), { ok: true, value: null });
  assert.equal(parseJsonValue('').ok, false);
  assert.equal(parseJsonValue('abc').ok, false);
});

test('formatJson tronque les gros résultats', () => {
  assert.equal(formatJson({ a: 1 }), '{\n  "a": 1\n}');
  assert.equal(formatJson(undefined), 'undefined');
  assert.ok(formatJson('x'.repeat(500), 100).length < 200);
});

test('historique : récent en premier, 20 maximum', () => {
  let h = [];
  for (let i = 0; i < 25; i++) h = pushHistory(h, { id: i, action: 'a', args: '{}', ms: 1, ok: true });
  assert.equal(h.length, 20);
  assert.equal(h[0].id, 24);
});

test('dossier parent', () => {
  assert.equal(parentDirectory('/a/b/studio.sqlite3'), '/a/b');
  assert.equal(parentDirectory('C:\\a\\b.db'), 'C:\\a');
  assert.equal(parentDirectory('x'), 'x');
});
