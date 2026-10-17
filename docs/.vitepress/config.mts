import { defineConfig } from 'vitepress'
import yl from './yl-grammar.json'

const reference = [
  ['Lexical syntax', 'lexical-syntax'],
  ['Modules and visibility', 'modules'],
  ['Nodes and captures', 'nodes'],
  ['Patterns and values', 'patterns'],
  ['Grammar operators', 'grammar-operators'],
  ['Pipes and rewrites', 'pipes'],
  ['Enums and arguments', 'enums'],
  ['Expression precedence', 'precedence'],
  ['Trivia and entry', 'trivia'],
  ['Checks and extensions', 'constraints'],
  ['Core and standard library', 'standard-library'],
  ['Semantic libraries', 'meaning-libraries'],
  ['Syntax examples', 'examples'],
  ['IDE metadata', 'ide'],
  ['Backend hooks', 'backends'],
].map(([text, slug]) => ({ text, link: `/reference/${slug}` }))

export default defineConfig({
  title: 'Your Language',
  description: 'Define grammars, shape syntax trees, and report precise diagnostics.',
  base: process.env.VITEPRESS_BASE || '/',
  cleanUrls: true,
  markdown: { languages: ['rust', 'javascript', 'json', 'shell', yl] },
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/introduction' },
      { text: 'Reference', link: '/reference/lexical-syntax' },
      { text: 'API', link: '/api/cli' },
      { text: 'Roadmap', link: '/roadmap' },
    ],
    sidebar: [
      { text: 'Guide', items: [
        { text: 'Introduction', link: '/guide/introduction' },
        { text: 'Installation', link: '/guide/installation' },
        { text: 'VS Code extension', link: '/guide/editor' },
      ] },
      { text: 'Tutorials', items: [
        { text: 'Learning path', link: '/tutorials/language-curriculum' },
        { text: 'Build a first language', link: '/tutorials/first-language' },
        { text: 'Modules and patterns', link: '/tutorials/modules-and-patterns' },
        { text: 'Composable definitions', link: '/tutorials/composable-definitions' },
        { text: 'Syntax diagnostics', link: '/tutorials/diagnostics' },
        { text: 'MiniJS walkthrough', link: '/tutorials/mini-js' },
        { text: 'Embed in Rust', link: '/tutorials/embedding' },
        { text: 'Interpret, transpile, and compile', link: '/tutorials/backends' },
        { text: 'Authored lexical scopes', link: '/tutorials/declarative-scopes' },
        { text: 'Build type inference with std/types', link: '/tutorials/std-types' },
        { text: 'Compound types', link: '/tutorials/compound-types' },
        { text: 'Diagnostic origins', link: '/tutorials/diagnostic-origins' },
        { text: 'Scopes and type inference', link: '/tutorials/semantic-inference' },
      ] },
      { text: 'Language reference', items: reference },
      { text: 'API reference', items: [
        { text: 'CLI commands', link: '/api/cli' },
        { text: 'Project configuration', link: '/api/projects' },
        { text: 'Rust API', link: '/api/rust' },
        { text: 'Backend dispatch', link: '/api/backends' },
        { text: 'Rust runtime operations', link: '/api/native-semantics' },
        { text: 'Authored semantic libraries', link: '/api/standard-semantics' },
        { text: 'Declarative attachment queries', link: '/api/declarative-queries' },
        { text: 'AST representation', link: '/api/ast' },
        { text: 'Diagnostics and spans', link: '/api/diagnostics' },
        { text: 'Compiled artifacts', link: '/api/artifacts' },
      ] },
      { text: 'Project', items: [
        { text: 'Roadmap', link: '/roadmap' },
        { text: 'Documentation development', link: '/guide/documentation' },
      ] },
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/gweiermann/your-language' }],
    editLink: {
      pattern: 'https://github.com/gweiermann/your-language/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    outline: [2, 3],
    footer: { message: 'Your Language Ã‚Â· Declarative syntax, structured trees, precise diagnostics.' },
  },
})
