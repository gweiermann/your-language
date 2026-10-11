const { copyFileSync, mkdirSync } = require('node:fs');
const { resolve } = require('node:path');
// The documentation grammar is the source of truth; packaged extensions are self-contained.
mkdirSync(resolve(__dirname, '../syntaxes'), { recursive: true });
copyFileSync(resolve(__dirname, '../../../docs/.vitepress/yl-grammar.json'), resolve(__dirname, '../syntaxes/yl.tmLanguage.json'));
