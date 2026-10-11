const { test } = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');

function setup(t, format) {
  let provider;
  const errors = [], logs = [], warnings = [], edits = [];
  const api = {
    workspace: { isTrusted: true, getConfiguration: () => ({ get: () => 'yl' }) },
    window: {
      createOutputChannel: () => ({ appendLine: text => logs.push(text), show() {}, dispose() {} }),
      showErrorMessage: async text => { errors.push(text); },
      showWarningMessage: async text => { warnings.push(text); },
    },
    languages: { registerDocumentFormattingEditProvider(selector, value) {
      assert.deepEqual(selector, [{ language: 'yl', scheme: 'file' }, { language: 'yl', scheme: 'untitled' }]);
      provider = value; return { dispose() {} };
    } },
    Range: class { constructor(start, end) { this.start = start; this.end = end; } },
    TextEdit: { replace: (range, text) => { const edit = { range, text }; edits.push(edit); return edit; } },
  };
  const originalLoad = Module._load;
  const formatter = require('../out/formatter');
  const originalFormat = formatter.formatBuffer;
  formatter.formatBuffer = format;
  Module._load = function(id, ...args) { return id === 'vscode' ? api : originalLoad.call(this, id, ...args); };
  const extension = require.resolve('../out/extension'); delete require.cache[extension];
  require(extension).activate({ subscriptions: [] });
  Module._load = originalLoad;
  t.after(() => { formatter.formatBuffer = originalFormat; delete require.cache[extension]; });
  const document = { uri: 'untitled:test.yl', version: 1, isClosed: false, getText: () => 'unsaved', positionAt: n => n };
  const token = { isCancellationRequested: false };
  return { api, document, token, run: () => provider.provideDocumentFormattingEdits(document, {}, token), errors, logs, warnings, edits };
}

test('provider returns one whole-buffer edit for an unsaved document', async t => {
  const env = setup(t, async source => { assert.equal(source, 'unsaved'); return 'formatted'; });
  const result = await env.run();
  assert.equal(result.length, 1);
  assert.equal(result[0].text, 'formatted');
  assert.equal(result[0].range.end, 7);
});

test('provider discards stale, cancelled, closed and unchanged results', async t => {
  for (const change of [env => env.document.version++, env => env.token.isCancellationRequested = true, env => env.document.isClosed = true]) {
    let env;
    env = setup(t, async () => { change(env); return 'formatted'; });
    assert.deepEqual(await env.run(), []);
  }
  const env = setup(t, async () => 'unsaved');
  assert.deepEqual(await env.run(), []);
});

test('errors are shown with details and do not edit the buffer', async t => {
  const env = setup(t, async () => { throw new Error('missing cli'); });
  assert.deepEqual(await env.run(), []);
  assert.equal(env.errors.length, 1);
  assert.deepEqual(env.logs, ['missing cli']);
  assert.equal(env.edits.length, 0);
});

test('untrusted workspace never launches the CLI', async t => {
  const env = setup(t, async () => { assert.fail('CLI launched'); });
  env.api.workspace.isTrusted = false;
  assert.deepEqual(await env.run(), []);
  assert.equal(env.warnings.length, 1);
});
