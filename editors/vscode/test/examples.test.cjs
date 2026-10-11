const { test } = require('node:test');
const assert = require('node:assert/strict');
const { resolve } = require('node:path');
const { documentPreviews, byteOffsetToPosition, loadPreviews } = require('../out/examples');
const token = { isCancellationRequested: false, onCancellationRequested: () => ({ dispose() {} }) };
test('previews filter missing examples and other modules, and preserve UTF-8 offsets', () => {
  const entry = resolve('language.yl');
  const items = [
    { definition: 'language.yl#Number', span: { file: 'language.yl', start: 0, end: 4 }, text: '42' },
    { definition: 'other.yl#Number', span: { file: 'other.yl', start: 0, end: 4 }, text: '42' },
    { definition: 'language.yl#NoExample', span: { file: 'language.yl', start: 0, end: 4 }, text: null },
  ];
  assert.equal(documentPreviews(items, entry, entry, 5).length, 1);
  assert.equal(documentPreviews(items, entry, entry, 3).length, 0);
  assert.equal(byteOffsetToPosition('é😀x', 6), 3);
});
test('previews are cancelled and reject relative CLI paths before launching', async () => {
  assert.deepEqual(await loadPreviews('yl', resolve('language.yl'), { ...token, isCancellationRequested: true }), []);
  await assert.rejects(loadPreviews('./yl', resolve('language.yl'), token), /absolute/);
});
test('real CLI provides validated example metadata', { skip: !process.env.YL_TEST_CLI }, async () => {
  const result = await loadPreviews(process.env.YL_TEST_CLI, resolve('../../examples/example_metadata/language.yl'), token);
  assert.equal(result.find(p => p.definition === 'language.yl#Program').text, 'let name=42;');
});
