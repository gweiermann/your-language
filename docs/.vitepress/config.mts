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
  ['Constraints and extensions', 'constraints'],
  ['Core and standard library', 'standard-library'],
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
      ] },
      { text: 'Tutorials', items: [
        { text: 'Build a first language', link: '/tutorials/first-language' },
        { text: 'Modules and patterns', link: '/tutorials/modules-and-patterns' },
        { text: 'Syntax diagnostics', link: '/tutorials/diagnostics' },
        { text: 'MiniJS walkthrough', link: '/tutorials/mini-js' },
        { text: 'Embed in Rust', link: '/tutorials/embedding' },
      ] },
      { text: 'Language reference', items: reference },
      { text: 'API reference', items: [
        { text: 'CLI commands', link: '/api/cli' },
        { text: 'Rust API', link: '/api/rust' },
        { text: 'Native semantic operations', link: '/api/native-semantics' },
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
    footer: { message: 'Your Language · Declarative syntax, structured trees, precise diagnostics.' },
  },
})
