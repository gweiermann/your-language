import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
const examples = [
  ['getting-started', 'language.yl', [['program.txt', true]]],
  ['constraints', 'language.yl', [['warning.txt', true], ['error.txt', false]]],
  ['mini-js', 'minijs.yl', [['program.js', true], ['invalid.js', false]]],
]

function run(args, success = true) {
  const result = spawnSync(cargo, ['run', '--quiet', '--locked', '--', ...args], { encoding: 'utf8' })
  if (result.error || result.status === null || (result.status === 0) !== success) {
    throw new Error(`Unexpected result for yl ${args.join(' ')}\n${result.error || ''}\n${result.stdout}\n${result.stderr}`)
  }
  return result
}

mkdirSync('target/docs-examples', { recursive: true })
for (const [directory, entry, inputs] of examples) {
  const artifact = `target/docs-examples/${directory}.ylc`
  run(['check', `examples/${directory}/${entry}`])
  run(['compile', `examples/${directory}/${entry}`, '-o', artifact])
  for (const [input, success] of inputs) {
    const checked = run(['language', artifact, 'check', `examples/${directory}/${input}`, '--json'], success)
    const diagnostics = JSON.parse(checked.stdout)
    if (!Array.isArray(diagnostics)) throw new Error('Expected diagnostic array')
    const parsed = run(['language', artifact, 'ast', `examples/${directory}/${input}`], success)
    JSON.parse(parsed.stdout)
  }
}
// Semantic consumers are separate from the syntax-only language CLI.
for (const [mode, success, expected] of [
  ['addition', true, 'numberSum: number'],
  ['higher-order', true, 'result: number'],
  ['errors', false, 'Error[query.diagnostic]'],
]) {
  const result = spawnSync(cargo, ['run', '--quiet', '--locked', '--example', 'mini_typescript', '--', mode], { encoding: 'utf8' })
  if (result.error || result.status === null || (result.status === 0) !== success ||
      !(result.stdout + result.stderr).includes(expected)) {
    throw new Error(`Unexpected Mini TypeScript ${mode} result\n${result.error || ''}\n${result.stdout}\n${result.stderr}`)
  }
}
const previews = JSON.parse(run(['examples', 'examples/example_metadata/language.yl']).stdout)
if (!previews.some(preview => preview.text === 'let name = 42 ;')) {
  throw new Error('Expected the documented whitespace-aware Program preview')
}
console.log('All documentation syntax, inference and example-preview flows passed.')
