const { test } = require('node:test');
const assert = require('node:assert/strict');
const { mkdtemp, writeFile, readFile, rm, readdir } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const { join } = require('node:path');
const { formatBuffer, FormattingCancelled } = require('../out/formatter');

function cancellation() {
  const listeners = new Set();
  return {
    isCancellationRequested: false,
    onCancellationRequested(listener) { listeners.add(listener); return { dispose: () => listeners.delete(listener) }; },
    cancel() { this.isCancellationRequested = true; for (const listener of listeners) listener(); },
  };
}

// NODE_OPTIONS loads a stub into a real child process, avoiding platform-specific scripts/shells.
async function stub(t, code) {
  const directory = await mkdtemp(join(tmpdir(), 'YL test spaces '));
  const hook = join(directory, 'hook.cjs');
  const original = process.env.NODE_OPTIONS;
  await writeFile(hook, code);
  process.env.NODE_OPTIONS = `--require "${hook.replaceAll('\\', '/')}"`;
  t.after(async () => {
    if (original === undefined) delete process.env.NODE_OPTIONS; else process.env.NODE_OPTIONS = original;
    await rm(directory, { recursive: true, force: true });
  });
  return directory;
}

test('formats an unsaved buffer via argument array; handles spaces, Unicode and large buffers', async t => {
  const directory = await stub(t, `const fs = require('node:fs');
    const assert = require('node:assert/strict');
    assert.equal(require('node:path').basename(process.argv[1]), 'fmt');
    assert.equal(process.argv[2], '--');
    const file = process.argv[3];
    fs.writeFileSync(file, fs.readFileSync(file, 'utf8') + '\\n');
    process.exit(0);`);
  // A saved file must not be touched when formatting the editor's divergent buffer.
  const saved = join(directory, 'language file.yl');
  await writeFile(saved, 'saved');
  const source = '// Grüße\n' + ' '.repeat(2 * 1024 * 1024);
  const result = await formatBuffer(source, process.execPath, cancellation());
  assert.equal(result, source + '\n');
  assert.equal(await readFile(saved, 'utf8'), 'saved');
});

test('failure returns diagnostics and cleans the temporary directory', async t => {
  await stub(t, `process.stderr.write('Error: invalid syntax'); process.exit(1);`);
  const before = (await readdir(tmpdir())).filter(name => name.startsWith('your-language-format-')).sort();
  await assert.rejects(formatBuffer('bad', process.execPath, cancellation()), /invalid syntax/);
  assert.deepEqual((await readdir(tmpdir())).filter(name => name.startsWith('your-language-format-')).sort(), before);
});

test('missing executable is actionable and never returns an edit', async () => {
  await assert.rejects(formatBuffer('node A="a"', join(tmpdir(), 'missing-yl-executable'), cancellation()), /yourLanguage.cliPath/);
});

test('cancellation before launch and during execution', async t => {
  const cancelled = cancellation(); cancelled.cancel();
  await assert.rejects(formatBuffer('buffer', process.execPath, cancelled), FormattingCancelled);
  await stub(t, `require('node:fs').writeFileSync(require('node:path').join(process.cwd(), 'fmt'), 'setTimeout(() => process.exit(0), 20000);');`);
  const token = cancellation();
  const pending = formatBuffer('buffer', process.execPath, token);
  setTimeout(() => token.cancel(), 100);
  await assert.rejects(pending, FormattingCancelled);
});

test('real yl fmt formats valid buffers and rejects malformed syntax', { skip: !process.env.YL_TEST_CLI }, async () => {
  const executable = process.env.YL_TEST_CLI;
  const input = 'node A="a" entry A';
  const expected = 'node A =\n    "a"\n\nentry A\n';
  assert.equal(await formatBuffer(input, executable, cancellation()), expected);
  assert.equal(await formatBuffer(expected, executable, cancellation()), expected);
  await assert.rejects(formatBuffer('node A = (', executable, cancellation()), /YL formatting failed/);
});
