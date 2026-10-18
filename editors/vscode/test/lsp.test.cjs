const {test} = require('node:test');
const assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const {mkdtemp, writeFile, rm} = require('node:fs/promises');
const {tmpdir} = require('node:os');
const {join, resolve} = require('node:path');
const {pathToFileURL, fileURLToPath} = require('node:url');
const rpc = require('vscode-jsonrpc/node');
const executable = process.env.YL_TEST_CLI || resolve(__dirname, '../../../target/debug/yl' + (process.platform === 'win32' ? '.exe' : ''));
async function session(t, entry, exampleDisplay = "inline") {
    const child=spawn(executable,['lsp'],{windowsHide:true});
    let stderr=''; child.stderr.on('data',chunk=>stderr+=chunk);
    const connection=rpc.createMessageConnection(new rpc.StreamMessageReader(child.stdout),new rpc.StreamMessageWriter(child.stdin));
    const notices=[],refreshes=[]; connection.onNotification('textDocument/publishDiagnostics',p=>notices.push(p));
    for(const method of ['workspace/codeLens/refresh','workspace/inlayHint/refresh']) connection.onRequest(method,()=>{refreshes.push(method);return null;});
    connection.listen();
    t.after(async()=> { try { await connection.sendRequest('shutdown'); await connection.sendNotification('exit'); await Promise.race([new Promise(resolve=>child.once('close',resolve)),new Promise((_,reject)=>{const timer=setTimeout(()=>reject(Error('Server did not exit with stdin open')),3000);timer.unref();})]); } finally { connection.dispose(); child.kill(); } });
    const init=await connection.sendRequest('initialize',{processId:process.pid,capabilities:{workspace:{codeLens:{refreshSupport:true},inlayHint:{refreshSupport:true}}},initializationOptions:{entry,...(exampleDisplay===null?{}:{exampleDisplay})}});
    assert.equal(init.capabilities.positionEncoding,'utf-16'); assert.equal(init.capabilities.textDocumentSync.change,1);
    connection.sendNotification('initialized',{});
    async function diagnostic(uri,version) {
        const limit=Date.now()+10000;
        while(Date.now()<limit) { const p=notices.findLast(p=>p.uri===uri&&p.version===version); if(p) return p; await new Promise(r=>setTimeout(r,10)); }
        throw Error('No diagnostics '+uri+' '+version+' '+stderr);
    }
    function open(uri,text,version=1) { connection.sendNotification('textDocument/didOpen',{textDocument:{uri,text,version,languageId:'yl'}}); }
    function change(uri,text,version) { connection.sendNotification('textDocument/didChange',{textDocument:{uri,version},contentChanges:[{text}]}); }
    async function hints(uri) { return connection.sendRequest('textDocument/inlayHint',{textDocument:{uri},range:{start:{line:0,character:0},end:{line:10000,character:10000}}}); }
    return {connection,open,change,diagnostic,hints,notices,refreshes};
}
test('real LSP client: unsaved imports, hints, diagnostics, formatting and close fallback',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'), helper=join(dir,'name.yl');
    await writeFile(helper,'export pattern Name = "disk"');
    const root=pathToFileURL(entry).href, dep=pathToFileURL(helper).href.replace(/^file:\/\/\/([A-Z]):/,(_,drive)=>`file:///${drive.toLowerCase()}:`);
    const s=await session(t,entry);
    s.open(root,'import { Name } from "./name"\nnode Root = Name\nentry Root'); await s.diagnostic(root,1);
    assert.match(JSON.stringify(await s.hints(root)),/disk/);
    s.open(dep,'// \u{1f600}\nexport pattern Name = "buffer"'); await s.diagnostic(dep,1);
    assert.match(JSON.stringify(await s.hints(root)),/buffer/);
    s.change(dep,'// \u{1f600}\nexport pattern Name = @',2);
    assert.ok((await s.diagnostic(dep,2)).diagnostics.some(d=>d.severity===1));
    assert.deepEqual(await s.hints(root),[]);
    s.change(dep,'export pattern Name="new"',3); await s.diagnostic(dep,3);
    assert.match(JSON.stringify(await s.hints(root)),/new/);
    const edits=await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri:dep},options:{tabSize:4,insertSpaces:true}});
    assert.equal(edits[0].newText,'export pattern Name = "new"\n');
    s.connection.sendNotification('textDocument/didClose',{textDocument:{uri:dep}});
    const limit=Date.now()+10000; while(Date.now()<limit) { if(JSON.stringify(await s.hints(root)).includes('disk')) break; await new Promise(r=>setTimeout(r,20)); }
    assert.match(JSON.stringify(await s.hints(root)),/disk/);
});
test('standard contracts validated without effects; incomplete untitled files and UTF16 diagnostics',async t=> {
    const s=await session(t);
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const uri=pathToFileURL(join(dir,'lang.yl')).href;
    s.open(uri,'import { createLexicalScope } from "std/references"\nnode Root = "x" { semantics { createLexicalScope(self) } }\nentry Root');
    assert.deepEqual((await s.diagnostic(uri,1)).diagnostics,[]);
    assert.match(JSON.stringify(await s.hints(uri)),/example/);
    s.change(uri,'import { missing } from "custom/native"\nnode Root="x"\nentry Root',2);
    assert.ok((await s.diagnostic(uri,2)).diagnostics.length);
    s.open('untitled:bad','node Root = "\u{1f600}" @');
    const diag=(await s.diagnostic('untitled:bad',1)).diagnostics[0]; assert.equal(diag.range.start.character,17);
});
test('cyclic graph terminates and newer document versions invalidate old results',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const a=join(dir,'a.yl'),b=join(dir,'b.yl'); const uri=pathToFileURL(a).href;
    await writeFile(b,'import { Root } from "./a"\nexport pattern Name = "x"');
    const s=await session(t,a); s.open(uri,'import { Name } from "./b"\nexport node Root = Name\nentry Root');
    await s.diagnostic(uri,1); assert.match(JSON.stringify(await s.hints(uri)),/example/);
    // Multiple versions are delivered back-to-back; this exercises protocol ordering,
    // not a compiler throughput benchmark on shared CI runners.
    const many=Array.from({length:400},(_,i)=>`pattern P${i}="x"`).join('\n')+'\nnode Root="x"\nentry Root';
    s.change(uri,many,2); s.change(uri,'node Root="latest"\nentry Root',3);
    await s.diagnostic(uri,3); assert.match(JSON.stringify(await s.hints(uri)),/latest/);
    s.change(uri,'node Root="stale"\nentry Root',2);
    await new Promise(r=>setTimeout(r,50)); assert.match(JSON.stringify(await s.hints(uri)),/latest/);
});

// Cancellation is sent through the same JSON-RPC client used by the extension.
test('cancellation suppresses a long formatting result',async t=> {
    const s=await session(t);
    const uri='untitled:large';
    const text=Array.from({length:12000},(_,i)=>`pattern P${i}="x"`).join('\n');
    s.open(uri,text); await s.diagnostic(uri,1);
    const token=new rpc.CancellationTokenSource();
    const request=s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}},token.token);
    token.cancel();
    await assert.rejects(request,error=>error.code===-32800);
});

test('above CodeLens defaults to declaration start, composes unsaved imports and switches without duplicates',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-lenses-'));t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'),helper=join(dir,'item.yl'); await writeFile(helper,'export node Item = "disk"');
    const uri=pathToFileURL(entry).href,dep=pathToFileURL(helper).href;
    const s=await session(t,entry,null);
    const lenses=()=>s.connection.sendRequest('textDocument/codeLens',{textDocument:{uri}});
    s.open(uri,'// \u{1f600}\nimport { Item } from "./item"\nexport node Root = "let" Item\ntrivia Space = / +/\nentry Root { trivia Space }'); await s.diagnostic(uri,1);
    const initial=await lenses(); assert.equal(initial.length,1);
    assert.ok(s.refreshes.includes('workspace/codeLens/refresh'));assert.ok(s.refreshes.includes('workspace/inlayHint/refresh'));
    assert.deepEqual(initial[0].range,{start:{line:2,character:0},end:{line:2,character:0}});
    assert.equal(initial[0].command.title,'example: let disk');assert.equal(initial[0].command.command,'yourLanguage.copyExample');assert.deepEqual(initial[0].command.arguments,['let disk']);
    assert.deepEqual(await s.hints(uri),[]);
    s.open(dep,'export node Item = "buffer"');await s.diagnostic(dep,1);assert.equal((await lenses())[0].command.title,'example: let buffer');
    await s.connection.sendNotification('workspace/didChangeConfiguration',{settings:{entry,exampleDisplay:'inline'}});
    assert.deepEqual(await lenses(),[]);assert.match(JSON.stringify(await s.hints(uri)),/buffer/);
    await s.connection.sendNotification('workspace/didChangeConfiguration',{settings:{entry,exampleDisplay:'off'}});
    assert.deepEqual(await lenses(),[]);assert.deepEqual(await s.hints(uri),[]);
    s.change(dep,'export node Item = @',2);assert.ok((await s.diagnostic(dep,2)).diagnostics.length);
    await s.connection.sendNotification('workspace/didChangeConfiguration',{settings:{entry,exampleDisplay:'above'}});
    assert.deepEqual(await lenses(),[]);
    s.change(dep,'export node Item = "x"',3);await s.diagnostic(dep,3);assert.equal((await lenses())[0].command.title,'example: let x');
});

test('CodeLens labels normalize and truncate text while copy retains complete example',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-lens-long-'));t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'),uri=pathToFileURL(entry).href;const text='a\n'+ 'b'.repeat(160);
    const s=await session(t,entry,null);s.open(uri,`node Root = ${JSON.stringify(text)}\nentry Root`);await s.diagnostic(uri,1);
    const [lens]=await s.connection.sendRequest('textDocument/codeLens',{textDocument:{uri}});
    assert.ok(lens.command.title.startsWith('example: a\\n'));assert.ok(lens.command.title.endsWith('\u2026'));assert.ok(!lens.command.title.includes('\n'));assert.equal(lens.command.arguments[0],text);

});

test('type contributions, syntax identities and authored wrappers validate without executing effects',async t=> {
    const s=await session(t);
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-types-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const uri=pathToFileURL(join(dir,'lang.yl')).href;
    const text=`import { error } from "core/diagnostics"
import * as types from "std/types"
import { children, first } from "core/query"
symbol number
operation annotateWithType(target,value) { types::annotateWithType(target,value) }
operation restrictTypeTo(target,value) { types::restrictTypeTo(target,value) }
operation typeof(value) = types::typeOf(value)
node Value="x" { semantics { annotateWithType(self,number) } }
node Root=value: Value { semantics {
    annotateWithType(self,typeof(value))
    restrictTypeTo(self,number)
    types::annotateWithType(value,[number])
    types::annotateWithType(first(children(self)),number)
    types::annotateWithType("invalid-key",number)
    match (types::typeOf(self)) {
        number => types::annotateWithType(self,number),
        _ => error("This runtime effect must not execute while checking the definition"),
    }
} }
entry Root`;
    s.open(uri,text);
    // Authored diagnostic branches must not execute during definition checks,
    // even while examples are being synthesized.
    assert.deepEqual((await s.diagnostic(uri,1)).diagnostics,[]);
    assert.match(JSON.stringify(await s.hints(uri)),/example/);
    const edits=await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}});
    assert.match(edits[0].newText,/import \* as types/);
    assert.match(edits[0].newText,/from "core\/query"/);
    assert.match(edits[0].newText,/operation annotateWithType\(target, value\)/);
    assert.match(edits[0].newText,/types::restrictTypeTo/);
    assert.match(edits[0].newText,/types::typeOf/);
    s.change(uri,text.replace('types::annotateWithType(target,value)','types::annotateWithType(target)'),2);
    assert.ok((await s.diagnostic(uri,2)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replace('types::restrictTypeTo(target,value)','types::restrictTypeTo(target)'),3);
    assert.ok((await s.diagnostic(uri,3)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replace('first(children(self))','first()'),4);
    assert.ok((await s.diagnostic(uri,4)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replaceAll('types::annotateWithType','types::ensure'),5);
    assert.ok((await s.diagnostic(uri,5)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text,6);
    assert.deepEqual((await s.diagnostic(uri,6)).diagnostics,[]);
    s.change(uri,text.replace('types::annotateWithType(target,value)','types::annotateWithType(self,value)'),7);
    assert.ok((await s.diagnostic(uri,7)).diagnostics.some(d=>d.code==='yl.operation_self'));
    s.change(uri,text,8);
    assert.deepEqual((await s.diagnostic(uri,8)).diagnostics,[]);
});

test('imported child CodeLens uses entry trivia and buffer changes preserve spaces',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-child-trivia-'));t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'),child=join(dir,'let.yl');
    await writeFile(entry,'import { Let } from "./let"\ntrivia Space = / +/\nnode Root = Let\nentry Root { trivia Space }');
    const text='pattern Name=/[a-z]+/ { ide { example "name" } }\nexport node Let = "let" Name "=" "42" ";"';
    await writeFile(child,text);
    const uri=pathToFileURL(child).href,s=await session(t,entry,null);
    s.open(uri,text);await s.diagnostic(uri,1);
    let lenses=await s.connection.sendRequest('textDocument/codeLens',{textDocument:{uri}});
    assert.ok(lenses.some(l=>l.command.title==='example: let name = 42 ;'));
    s.change(uri,text.replace('example "name"','example "other"'),2);await s.diagnostic(uri,2);
    lenses=await s.connection.sendRequest('textDocument/codeLens',{textDocument:{uri}});
    assert.ok(lenses.some(l=>l.command.title==='example: let other = 42 ;'));
});

test('LSP buffer formatting aligns match arrows by default',async t=> {
    const s=await session(t),uri='untitled:alignment';
    const text='node Root = "x" { semantics { match (a,b) { number,number => type(number), string,string | string,number | number,string => type(string), _ => {} } } }';
    s.open(uri,text);await s.diagnostic(uri,1);
    const [edit]=await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}});
    const lines=edit.newText.split('\n').filter(line=>line.includes('=>'));
    assert.equal(lines.length,3);assert.equal(new Set(lines.map(line=>line.indexOf('=>'))).size,1);
});


test('project discovery analyzes matching programs, applies contextual tokens and reloads unsaved definitions/config',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-project~lsp-'));t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'),config=join(dir,'yl.json'),program=join(dir,'program.ts');
    const definition=`import { error } from "core/diagnostics"
pattern Name = /[a-z]+/ { ide { highlightAs "variable" } }
pattern FunctionName = Name { ide { highlightAs "function" } }
pattern FunctionKeyword = "function" { ide { highlightAs "keyword" } }
node Number = /[0-9]+/ { ide { example "42" highlightAs "number" } }
node Root = FunctionKeyword name:FunctionName value:Number { checks { when name == "bad" { error("Reserved name") } } }
trivia Space = / +/
entry Root { trivia Space }`;
    await writeFile(entry,definition);await writeFile(config,JSON.stringify({entry:'./language.yl',files:['*.ts']}));
    const uri=pathToFileURL(program).href,entryUri=pathToFileURL(entry).href,configUri=pathToFileURL(config).href;
    const s=await session(t);
    assert.deepEqual(await s.connection.sendRequest('yl/project',{textDocument:{uri}}),{matches:true});
    assert.deepEqual(await s.connection.sendRequest('yl/project',{textDocument:{uri:pathToFileURL(join(dir,'outside.js')).href}}),{matches:false});
    s.open(uri,'function foo 42');assert.deepEqual((await s.diagnostic(uri,1)).diagnostics,[]);
    const tokens=()=>s.connection.sendRequest('textDocument/semanticTokens/full',{textDocument:{uri}});
    assert.deepEqual((await tokens()).data,[0,0,8,15,0,0,9,3,12,0,0,4,2,19,0]);
    assert.equal(await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}}),null);
    s.change(uri,'function foo ?',2);assert.ok((await s.diagnostic(uri,2)).diagnostics.some(d=>d.code.startsWith('parse.')));
    s.change(uri,'function foo 42',3);assert.deepEqual((await s.diagnostic(uri,3)).diagnostics,[]);
    s.change(uri,'function bad 42',4);const reserved=(await s.diagnostic(uri,4)).diagnostics.find(d=>d.code==='parse.constraint');assert.equal(fileURLToPath(reserved.relatedInformation[0].location.uri),entry);
    s.change(uri,'function foo 42',5);assert.deepEqual((await s.diagnostic(uri,5)).diagnostics,[]);
    s.open(entryUri,definition.replace('highlightAs "function"','highlightAs "property"'));await s.diagnostic(entryUri,1);
    assert.equal((await tokens()).data[8],9);
    s.open(configUri,JSON.stringify({entry:'./missing.yl',files:['*.ts']}));
    await s.diagnostic(configUri,1);
    assert.ok(s.notices.some(n=>n.diagnostics.some(d=>d.code==='yl.io')));
    s.change(configUri,JSON.stringify({entry:'./language.yl',files:['*.js']}),2);await s.diagnostic(configUri,2);
    assert.deepEqual(await s.connection.sendRequest('yl/project',{textDocument:{uri}}),{matches:false});
    assert.deepEqual((await tokens()).data,[]);
    s.change(configUri,'broken',3);assert.ok((await s.diagnostic(configUri,3)).diagnostics.some(d=>d.code==='yl.project'));
});

test('cached program results survive reopen and invalidate on closed import disk edits', async t => {
    const dir = await mkdtemp(join(tmpdir(), 'yl-reopen~lsp-'));
    t.after(() => rm(dir, {recursive: true, force: true}));
    const entry = join(dir, 'language.yl'), helper = join(dir, 'value.yl');
    await writeFile(join(dir, 'yl.json'), JSON.stringify({entry: 'language.yl', files: ['*.txt']}));
    await writeFile(entry, 'import { Value } from "./value"\nnode Root = Value\nentry Root');
    await writeFile(helper, 'export pattern Value = /[0-9]+/ { ide { highlightAs "number" } }');
    const uri = pathToFileURL(join(dir, 'program.txt')).href;
    const dep = pathToFileURL(helper).href;
    const s = await session(t);
    const tokens = () => s.connection.sendRequest('textDocument/semanticTokens/full', {textDocument: {uri}});
    s.open(uri, '42');
    assert.deepEqual((await s.diagnostic(uri, 1)).diagnostics, []);
    const initial = await tokens();
    assert.ok(initial.data.length);
    await s.connection.sendNotification('textDocument/didClose', {textDocument: {uri}});
    assert.deepEqual((await tokens()).data, []);
    s.open(uri, '42', 2);
    assert.deepEqual((await s.diagnostic(uri, 2)).diagnostics, []);
    assert.deepEqual(await tokens(), initial);
    s.open(dep, 'export pattern Value = /[0-9]+/ { ide { highlightAs "string" } }');
    await s.diagnostic(dep, 1);
    assert.notDeepEqual(await tokens(), initial);
    await s.connection.sendNotification('textDocument/didClose', {textDocument: {uri: dep}});
    assert.deepEqual(await tokens(), initial);
    await writeFile(helper, 'export pattern Value = /[a-z]+/');
    await s.connection.sendNotification('workspace/didChangeWatchedFiles', {changes: [{uri: dep, type: 2}]});
    // The token request is a barrier for watched-file processing.
    assert.deepEqual((await tokens()).data, []);
    assert.ok(s.notices.findLast(p => p.uri === uri).diagnostics.some(d => d.code.startsWith('parse.')));
});


test('declaration documentation hover uses unsaved buffers and leaves target programs alone', async t => {
    const s = await session(t);
    const uri = 'untitled:documented.yl';
    const source = '/// Matches a decimal number.\n///\n/// Retains its text.\nexport node Number = /[0-9]+/\nentry Number\n';
    s.open(uri, source);
    await s.diagnostic(uri, 1);
    const hover = await s.connection.sendRequest('textDocument/hover', {
        textDocument: {uri}, position: {line: 3, character: 14},
    });
    assert.equal(hover.contents[1], 'Matches a decimal number.\n\nRetains its text.');
    assert.equal(hover.range.start.character, 12);
    assert.equal(await s.connection.sendRequest('textDocument/hover', {
        textDocument: {uri}, position: {line: 4, character: 8},
    }), null);
});
