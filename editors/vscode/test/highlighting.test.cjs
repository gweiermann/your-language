const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { Registry, parseRawGrammar, INITIAL } = require('vscode-textmate');
const oniguruma = require('vscode-oniguruma');

test('packaged grammar equals docs source and tokenizes representative YL syntax', async () => {
  const source = readFileSync(join(__dirname, '../../../docs/.vitepress/yl-grammar.json'), 'utf8');
  assert.equal(readFileSync(join(__dirname, '../syntaxes/yl.tmLanguage.json'), 'utf8'), source);
  await oniguruma.loadWASM(readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')).buffer);
  const registry = new Registry({
    onigLib: Promise.resolve({ createOnigScanner: patterns => new oniguruma.OnigScanner(patterns), createOnigString: text => new oniguruma.OnigString(text) }),
    loadGrammar: async () => parseRawGrammar(source, 'yl.tmLanguage.json'),
  });
  const grammar = await registry.loadGrammar('source.yl');
  function scope(text, word, expected) {
    const offset = text.indexOf(word);
    const result = grammar.tokenizeLine(text, INITIAL);
    const token = result.tokens.find(token => token.startIndex <= offset && token.endIndex > offset);
    assert.ok(token.scopes.includes(expected), `${word}: ${token.scopes}`);
  }
  scope('export node Number = value: /[0-9]+/', 'node', 'keyword.control.yl');
  scope('export node Number = value: /[0-9]+/', 'Number', 'entity.name.type.yl');
  scope('export node Number = value: /[0-9]+/', 'value', 'entity.name.tag.yl');
  scope('pattern A = /a\\/b/ |> wrap(.A)', '/a', 'string.regexp.yl');
  scope('pattern A = /a\\/b/ |> wrap(.A)', '|>', 'keyword.operator.yl');
  scope('pattern A = /a\\/b/ |> wrap(.A)', '.A', 'constant.other.enum.yl');
  scope('meanings { boundary match (typeof(left), typeof(right)) {} }', 'boundary', 'keyword.control.yl');
  scope('meanings { boundary match (typeof(left), typeof(right)) {} }', 'match', 'keyword.control.yl');
  scope('meanings { boundary match (typeof(left), typeof(right)) {} }', 'typeof', 'entity.name.function.yl');
  scope('// node fake', 'node', 'comment.line.double-slash.yl');
  scope('"node \\"literal\\""', 'node', 'string.quoted.double.yl');
  registry.dispose();
});
