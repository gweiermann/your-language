import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { basename, dirname, resolve, sep } from 'node:path'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'

// Materialize the actual full-file fences in chapter order. Every checkpoint
// gets its own directory, so later chapters cannot hide an earlier broken step.
const repository = resolve('.')
const runs = resolve('target/docs-course')
mkdirSync(runs, { recursive: true })
const output = mkdtempSync(resolve(runs, 'run-'))
const chapters = resolve('docs/tutorials/course')
const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
function execute(args, expected = 0, cwd = repository) {
  const result = spawnSync(cargo, ['run', '--quiet', '--locked', '--manifest-path', resolve(repository, 'Cargo.toml'), '--', ...args], { encoding: 'utf8', cwd })
  if (result.error || result.status !== expected) {
    throw new Error(`yl ${args.join(' ')}: expected ${expected}, got ${result.status}\n${result.error || ''}\n${result.stdout}\n${result.stderr}`)
  }
  return result
}
function run(args, expected = 0, cwd = repository) {
  return execute(args, expected, cwd).stdout
}
function inspectEarlyStep(label, directory) {
  run(['compile', 'definition/language.yl', '-o', 'language.ylc'], 0, directory)
  if (label === 'rejected-number') {
    const result = execute(['language', 'language.ylc', 'check', 'programs/program.txt'], 1, directory)
    assert.match(result.stderr, /Error\[parse.unexpected_token\]: Expected \/\[0-9\]\+\//)
    assert.match(result.stderr, /1 \| hello/)
    return
  }
  const ast = JSON.parse(run(['language', 'language.ylc', 'ast', 'programs/program.txt'], 0, directory))
  assert.deepEqual(ast, { type: 'Number', fields: label === 'number' ? {} : {value: '42'},
    span: { file: 'programs/program.txt', start: 0, end: 2 } })
}
function write(directory, name, text) {
  const path = resolve(directory, name)
  if (!path.startsWith(directory + sep)) throw new Error(`Invalid course path: ${name}`)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, text)
}
function exercise(directory, name, change, expected, message) {
  const path = resolve(directory, name)
  const original = readFileSync(path, 'utf8')
  const modified = change(original)
  assert.notEqual(modified, original, `Exercise did not change ${name}`)
  try {
    writeFileSync(path, modified)
    const diagnostics = JSON.parse(run(['project', 'check', name, '--json'], expected, directory))
    if (message) assert.match(diagnostics.map(item => item.message).join('\n'), message)
  } finally { writeFileSync(path, original) }
}
let previous
let expectedErrors = new Set()
let bundled = false
let count = 0
for (const file of readdirSync(chapters).filter(name => /^\d.*\.md$/.test(name)).sort()) {
  const markdown = readFileSync(resolve(chapters, file), 'utf8')
  const directory = resolve(output, basename(file, '.md'))
  const checkpoint = markdown.match(/<!-- course:project=([^\s]+) -->/)
  if (checkpoint) {
    bundled = true
    const source = resolve(repository, checkpoint[1])
    for (const name of ['definition', 'programs', 'yl.json']) cpSync(resolve(source, name), resolve(directory, name), { recursive: true })
    expectedErrors = new Set(JSON.parse(readFileSync(resolve(directory, 'yl.json'), 'utf8')).expectErrors || [])
  } else if (previous) cpSync(previous, directory, { recursive: true })
  const events = /```(?:yl|json|text) file=(\S+)([^\n]*)\r?\n([\s\S]*?)```|<!-- course:checkpoint=(\S+) -->/g
  for (const match of markdown.matchAll(events)) {
    if (match[4]) { inspectEarlyStep(match[4], directory); continue }
    const [, name, metadata, body] = match
    const text = metadata.includes('no-final-newline') ? body.replace(/\r?\n$/, '') : body
    write(directory, name, text)
    if (metadata.includes('expect-error')) expectedErrors.add(name)
    else expectedErrors.delete(name)
  }
  const config = JSON.parse(readFileSync(resolve(directory, 'yl.json'), 'utf8'))
  // Expected-error fences are documentation verification metadata; they do not
  // silently modify the configuration shown to the reader.
  run(['check', resolve(directory, config.entry)])
  if (bundled) {
    const checked = JSON.parse(run(['project', 'check', directory, '--json']))
    if (!checked.success) throw new Error(`Checkpoint failed: ${file}`)
  } else for (const program of readdirSync(resolve(directory, 'programs')).filter(name => name.endsWith('.txt'))) {
    const name = `programs/${program}`
    const diagnostics = JSON.parse(run(['project', 'check', resolve(directory, name), '--json'], expectedErrors.has(name) ? 1 : 0))
    if (!Array.isArray(diagnostics)) throw new Error(`Expected diagnostic array for ${file}/${program}`)
    if (program === 'invalid.txt' && count === 2) {
      assert.equal(diagnostics.length, 1)
      const diagnostic = diagnostics[0]
      assert.match(diagnostic.message, /Choose a name other than 'reserved'/)
      assert.equal(diagnostic.primary.start, 4)
      assert.equal(diagnostic.primary.end, 12)
      const human = execute(['project', 'check', name], 1, directory).stderr
      assert.match(human, /let reserved = 42;/)
      assert.match(human, /\^{8}/)
    }
    if (program === 'invalid.txt' && count === 4) {
      assert.match(diagnostics[0].message, /Type .*string.* does not match .*number/)
    }
  }
  if (count === 0) {
    const artifact = resolve(directory, 'language.ylc')
    run(['compile', resolve(directory, config.entry), '-o', artifact])
    JSON.parse(run(['language', artifact, 'ast', resolve(directory, 'programs/program.txt')]))
  }
  if (count === 2) exercise(directory, 'programs/invalid.txt', text => text.replace('reserved', 'available'), 0)
  if (count === 3) exercise(directory, 'programs/program.txt', text => text.replace('count +', 'missing +'), 1, /Unknown name/)
  if (count === 4) exercise(directory, 'programs/invalid.txt', text => text.replace(': number', ': string'), 0)
  if (count === 5) exercise(directory, 'programs/course-inference.txt', text => text.replace('identity(42)', 'identity(42, 1)'), 1, /argument/i)
  if (count === 6) {
    exercise(directory, 'programs/course-structure.txt', text => text.replace('box.value', 'box.missing'), 1, /property|field/i)
    exercise(directory, 'programs/invalid-course.txt', text => text.replace('value: 42', 'value: "hello"'), 0)
  }
  previous = directory
  count++
}
for (const name of ['docs/api/core.md', 'docs/guide/queries.md', 'docs/guide/host-hooks.md', 'docs/tutorials/course/08-editor.md']) {
  const markdown = readFileSync(name, 'utf8')
  for (const [, label, source] of markdown.matchAll(/```yl verify=(\S+)\r?\n([\s\S]*?)```/g)) {
    const file = `${basename(name, '.md')}-${label}.yl`
    write(output, file, source)
    run(['check', resolve(output, file)])
  }
}
// Every bundled public export needs an entry in the canonical reference.
const reference = readFileSync('docs/reference/standard-library.md', 'utf8')
for (const file of ['parser.yl', 'references.yl', 'types.yl', 'types/arithmetic.yl', 'types/structural.yl', 'backend/integer.yl']) {
  for (const [, name] of readFileSync(resolve('stdlib', file), 'utf8').matchAll(/export (?:operation|symbol|enum|pipe|hook) (\w+)/g)) {
    if (!reference.includes(name)) throw new Error(`Undocumented standard export: ${file} ${name}`)
  }
}
for (const file of readdirSync(chapters)) {
  const markdown = readFileSync(resolve(chapters, file), 'utf8')
  for (const [, include] of markdown.matchAll(/^<<< (\S+)/gm)) {
    if (!existsSync(resolve(chapters, include))) throw new Error(`Missing course source include: ${include}`)
  }
  for (const [, path, operation, shown] of markdown.matchAll(/```yl source=(\S+)#(\w+)\r?\n([\s\S]*?)```/g)) {
    const source = readFileSync(resolve(repository, path), 'utf8')
    const declaration = new RegExp(`^(?:export )?operation ${operation}\\b`, 'm').exec(source)
    if (!declaration) throw new Error(`Missing source operation ${path}#${operation}`)
    const next = /\n(?:export )?(?:operation|symbol|enum|node|pattern) /.exec(source.slice(declaration.index + declaration[0].length))
    const end = next ? declaration.index + declaration[0].length + next.index : source.length
    if (source.slice(declaration.index, end).replace(/\s+/g, '') !== shown.replace(/\s+/g, '')) {
      throw new Error(`Course excerpt differs from its maintained source: ${path}#${operation}`)
    }
  }
}
let readmeSnippet = 0
for (const [, source] of readFileSync('readme.md', 'utf8').matchAll(/```yl\r?\n([\s\S]*?)```/g)) {
  const path = `readme-${readmeSnippet++}.yl`
  write(output, path, source)
  run(['check', resolve(output, path)])
}
// Only this invocation's generated child directory is disposable. Leave failed
// runs available for diagnosis, and never delete project/example source trees.
if (!output.startsWith(runs + sep) || !basename(output).startsWith('run-')) throw new Error('Unsafe course cleanup path')
rmSync(output, { recursive: true })
console.log(`Checked ${count} sequential course checkpoints, standalone examples, artifact reload, and bundled-library exports.`)
