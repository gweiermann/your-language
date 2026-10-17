import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'


function run(args, success = true) {
  const result = spawnSync(cargo, ['run', '--quiet', '--locked', '--', ...args], { encoding: 'utf8' })
  if (result.error || result.status === null || (result.status === 0) !== success) {
    throw new Error(`Unexpected result for yl ${args.join(' ')}\n${result.error || ''}\n${result.stdout}\n${result.stderr}`)
  }
  return result
}

mkdirSync('target/docs-examples', { recursive: true })
// The catalog is discovered from the same yl.json files used by CLI and editor.
const projects = JSON.parse(run(['project', 'list', 'examples', '--json']).stdout)
if (projects.length !== 14) throw new Error('Expected all 14 numbered example projects')
for (const project of projects) {
  const checked = JSON.parse(run(['project', 'check', project.config, '--json']).stdout)
  if (!checked.success || !checked.sources.length) throw new Error(`Project failed: ${project.config}`)
}
// Exercise persisted artifacts independently of project compilation.
const artifact = 'target/docs-examples/minijs.ylc'
run(['compile', 'examples/03-mini-js/definition/minijs.yl', '-o', artifact])
for (const [input, success] of [['program', true], ['invalid', false]]) {
  const source = `examples/03-mini-js/programs/${input}.txt`
  JSON.parse(run(['language', artifact, 'check', source, '--json'], success).stdout)
  JSON.parse(run(['language', artifact, 'ast', source], success).stdout)
}
// Semantic consumers are separate from the syntax-only language CLI.
for (const [mode, success, expected] of [
  ['addition', true, 'numberSum: Type::number'],
  ['higher-order', true, 'result: Type::number'],
  ['errors', false, 'Error[query.diagnostic]'],
]) {
  const result = spawnSync(cargo, ['run', '--quiet', '--locked', '--example', 'mini_typescript_host', '--', mode], { encoding: 'utf8' })
  if (result.error || result.status === null || (result.status === 0) !== success ||
      !(result.stdout + result.stderr).includes(expected)) {
    throw new Error(`Unexpected Mini TypeScript ${mode} result\n${result.error || ''}\n${result.stdout}\n${result.stderr}`)
  }
}
const previews = JSON.parse(run(['examples', 'examples/04-ide-metadata/definition/language.yl']).stdout)
if (!previews.some(preview => preview.text === 'let name = 42 ;')) {
  throw new Error('Expected the documented whitespace-aware Program preview')
}
console.log('All documentation syntax, inference and example-preview flows passed.')

for (const [source, success] of [
  ['examples/08-mini-typescript/programs/program.txt', true],
  ['examples/11-compound-types/programs/valid.txt', true],
  ['examples/12-diagnostic-origins/programs/invalid-scattered.txt', false],
]) {
  const checked = run(['project', 'check', source, '--json'], success)
  if (!Array.isArray(JSON.parse(checked.stdout))) throw new Error('Expected project diagnostic array')
}
JSON.parse(run(['project', 'ast', 'examples/08-mini-typescript/programs/program.txt']).stdout)
