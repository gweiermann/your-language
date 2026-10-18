// Run from the repository root after npm --prefix editors/vscode ci.
// node scripts/benchmark-lsp.cjs <yl executable> [project names...] [--max-warm-open-ms=250]
const {spawn} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const rpc = require('../editors/vscode/node_modules/vscode-jsonrpc/node');

async function benchmark(executable, project) {
    const directory = path.resolve('examples', project, 'programs');
    const files = fs.readdirSync(directory).filter(file => file.endsWith('.txt'));
    const file = files.includes('valid.txt') ? 'valid.txt' : files.includes('program.txt') ? 'program.txt' : files[0];
    const uri = pathToFileURL(path.join(directory, file)).href;
    const other = pathToFileURL(path.join(directory, '__lsp-benchmark__.txt')).href;
    const text = fs.readFileSync(path.join(directory, file), 'utf8');
    const child = spawn(executable, ['lsp'], {windowsHide: true});
    let stderr = '';
    child.stderr.on('data', chunk => stderr += chunk);
    const connection = rpc.createMessageConnection(new rpc.StreamMessageReader(child.stdout), new rpc.StreamMessageWriter(child.stdin));
    let waiter;
    connection.onNotification('textDocument/publishDiagnostics', params => {
        if (waiter && params.uri === waiter.uri && params.version === waiter.version) {
            waiter.resolve(params);
            waiter = undefined;
        }
    });
    child.on('error', () => connection.dispose());
    connection.listen();
    try {
        await connection.sendRequest('initialize', {
            processId: process.pid,
            capabilities: {},
            initializationOptions: {exampleDisplay: 'off'}
        });
        connection.sendNotification('initialized', {});
        const results = [];
        async function measure(step, document, version, method, content) {
            const start = performance.now();
            const notification = new Promise((resolve, reject) => {
                const timer = setTimeout(() => reject(Error(`Diagnostics timed out: ${stderr}`)), 120000);
                waiter = {uri: document, version, resolve: params => {clearTimeout(timer); resolve(params);}};
            });
            connection.sendNotification(method, method === 'textDocument/didOpen'
                ? {textDocument: {uri: document, text: content, version, languageId: 'yl-program'}}
                : {textDocument: {uri: document, version}, contentChanges: [{text: content}]});
            const diagnostics = await notification;
            const diagnosticsMs = performance.now() - start;
            const tokens = await connection.sendRequest('textDocument/semanticTokens/full', {textDocument: {uri: document}});
            results.push({step, diagnosticsMs: Math.round(diagnosticsMs), tokensReadyMs: Math.round(performance.now() - start), diagnostics: diagnostics.diagnostics.length, tokenIntegers: tokens.data.length});
        }
        await measure('cold-open', uri, 1, 'textDocument/didOpen', text);
        connection.sendNotification('textDocument/didClose', {textDocument: {uri}});
        // A request is a processing barrier for the preceding close notification.
        await connection.sendRequest('textDocument/semanticTokens/full', {textDocument: {uri}});
        await measure('warm-reopen', uri, 2, 'textDocument/didOpen', text);
        await measure('source-edit', uri, 3, 'textDocument/didChange', text + '\n');
        await measure('second-open', other, 1, 'textDocument/didOpen', text);
        await measure('unchanged-update-two-open', other, 2, 'textDocument/didChange', text);
        const start = performance.now();
        for (let i = 0; i < 10; i++) {
            await connection.sendRequest('textDocument/semanticTokens/full', {textDocument: {uri}});
        }
        results.push({step: 'ten-token-requests', tokensReadyMs: Math.round(performance.now() - start)});
        await connection.sendRequest('shutdown');
        await connection.sendNotification('exit');
        await new Promise(resolve => child.once('close', resolve));
        return {project, results};
    } finally {
        connection.dispose();
        if (!child.stdin.destroyed) child.stdin.end();
        if (child.exitCode === null) child.kill();
    }
}

(async () => {
    const [executable, ...args] = process.argv.slice(2);
    if (!executable) throw Error('Usage: node scripts/benchmark-lsp.cjs <yl executable> [project names...] [--max-warm-open-ms=250]');
    const limit = args.find(arg => arg.startsWith('--max-warm-open-ms='));
    const projects = args.filter(arg => !arg.startsWith('--'));
    if (!projects.length) projects.push('01-getting-started', '08-mini-typescript', '11-compound-types', '12-diagnostic-origins', '14-backend-hooks');
    for (const project of projects) {
        const result = await benchmark(path.resolve(executable), project);
        console.log(JSON.stringify(result));
        if (limit && result.results.find(row => row.step === 'warm-reopen').tokensReadyMs > Number(limit.split('=')[1])) process.exitCode = 1;
    }
})().catch(error => {console.error(error); process.exitCode = 2;});
