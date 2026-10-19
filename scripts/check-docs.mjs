import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { resolve, dirname, relative } from 'node:path'

const root = resolve('docs/.vitepress/dist')
const base = (process.env.VITEPRESS_BASE || '/').replace(/\/$/, '')
function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name)
    return entry.isDirectory() ? files(path) : path.endsWith('.html') ? [path] : []
  })
}
const pages = files(root)
const errors = []
for (const page of pages) {
  const html = readFileSync(page, 'utf8')
  for (const [, raw] of html.matchAll(/href="([^"]+)"/g)) {
    if (/^(?:https?:|mailto:|tel:|data:)/.test(raw) || raw.startsWith('//')) continue
    const [url, anchor] = raw.replace(/&amp;/g, '&').split('#')
    const path = decodeURIComponent(url.split('?')[0])
    let target = path === '' ? page : path.startsWith('/')
      ? resolve(root, `.${base && path.startsWith(`${base}/`) ? path.slice(base.length) : path}`)
      : resolve(dirname(page), path)
    if (!existsSync(target) || !target.match(/\.[a-z0-9]+$/i)) {
      const candidates = [target + '.html', resolve(target, 'index.html')]
      target = candidates.find(existsSync) || target
    }
    if (!existsSync(target)) {
      errors.push(`${relative(root, page)} â†’ missing ${raw}`)
    } else if (anchor && target.endsWith('.html') && !readFileSync(target, 'utf8').includes(`id="${decodeURIComponent(anchor)}"`)) {
      errors.push(`${relative(root, page)} â†’ missing anchor ${raw}`)
    }
  }
}
if (errors.length) throw new Error(errors.join('\n'))
// Fence metadata is not rendered by VitePress. Require real, adjacent filename
// content in built pages so source-only checks cannot miss invisible captions.
for (const name of readdirSync('docs/tutorials/course').filter(name => /^\d.*\.md$/.test(name))) {
  const markdown = readFileSync(resolve('docs/tutorials/course', name), 'utf8')
  const html = readFileSync(resolve(root, 'tutorials/course', name.replace(/\.md$/, '.html')), 'utf8')
  for (const match of markdown.matchAll(/(?:```(?:yl|json|text) (?:file|source)=(\S+)|^<<< \.\.\/\.\.\/\.\.\/(\S+))/gm)) {
    const path = (match[1] || match[2]).split('#')[0]
    const prefix = markdown.slice(0, match.index).trimEnd()
    const caption = prefix.match(/<p class="course-file" data-file="([^"]+)">[^\n]*<\/code><\/p>$/)
    if (caption?.[1] !== path) throw new Error(`Missing visible filename before ${name}: ${path}`)
    const escaped = path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const rendered = new RegExp(String.raw`<p class="course-file" data-file="${escaped}">[\s\S]*?<code>${escaped}</code></p>\s*<div class="language-`)
    if (!rendered.test(html)) throw new Error(`Filename not rendered next to code: ${name}: ${path}`)
  }
}
console.log(`Checked visible course filenames, internal links and anchors across ${pages.length} HTML pages.`)
