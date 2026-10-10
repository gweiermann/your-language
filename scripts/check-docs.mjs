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
      errors.push(`${relative(root, page)} → missing ${raw}`)
    } else if (anchor && target.endsWith('.html') && !readFileSync(target, 'utf8').includes(`id="${decodeURIComponent(anchor)}"`)) {
      errors.push(`${relative(root, page)} → missing anchor ${raw}`)
    }
  }
}
if (errors.length) throw new Error(errors.join('\n'))
console.log(`Checked internal links and anchors across ${pages.length} HTML pages.`)
