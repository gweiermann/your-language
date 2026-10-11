const { test } = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const { resolve } = require('node:path');
function setup(t, load) {
  let provider;
  const logs = [];
  const entry = resolve('language.yl');
  const api = {
    workspace: { isTrusted: true, getConfiguration: () => ({ get: key => key === 'exampleEntry' ? entry : 'yl' }) },
    window: { createOutputChannel: () => ({ appendLine: text => logs.push(text), dispose() {} }) },
    languages: { registerDocumentFormattingEditProvider: () => ({ dispose() {} }), registerInlayHintsProvider: (_selector, value) => { provider = value; return { dispose() {} }; } },
    InlayHint: class { constructor(position, label) { this.position = position; this.label = label; } },
    InlayHintKind: { Type: 1 },
  };
  const examples = require('../out/examples');
  const original = examples.loadPreviews;
  examples.loadPreviews = load;
  const originalLoad = Module._load;
  Module._load = function(id, ...args) { return id === 'vscode' ? api : originalLoad.call(this, id, ...args); };
  const extension = require.resolve('../out/extension'); delete require.cache[extension];
  require(extension).activate({ subscriptions: [] });
  Module._load = originalLoad;
  t.after(() => { examples.loadPreviews = original; delete require.cache[extension]; });
  const document = { uri: { fsPath: entry }, version: 1, isClosed: false, isDirty: false, getText: () => 'é😀x', positionAt: n => n };
  const token = { isCancellationRequested: false };
  return { api, entry, document, token, logs, run: () => provider.provideInlayHints(document, { contains: () => true }, token) };
}
test('inline previews convert byte offsets and preserve escaped text', async t => {
  const env = setup(t, async () => [{ definition: 'language.yl#Root', span: { file: 'language.yl', start: 0, end: 6 }, text: 'x\ny' }]);
  const hints = await env.run();
  assert.equal(hints[0].position, 3);
  assert.equal(hints[0].label, ' example: "x\\ny"');
});
test('dirty and untrusted modules never launch preview CLI', async t => {
  const env = setup(t, async () => assert.fail('CLI launched'));
  env.document.isDirty = true; assert.deepEqual(await env.run(), []);
  env.document.isDirty = false; env.api.workspace.isTrusted = false; assert.deepEqual(await env.run(), []);
});
test('stale results and compiler failures yield no inline hints', async t => {
  let env;
  env = setup(t, async () => { env.document.version++; return []; });
  assert.deepEqual(await env.run(), []);
  const failed = setup(t, async () => { throw new Error('invalid example'); });
  assert.deepEqual(await failed.run(), []);
  assert.match(failed.logs[0], /invalid example/);
});
