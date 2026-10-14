const {test} = require('node:test');
const assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const {mkdtemp, writeFile, rm} = require('node:fs/promises');
const {tmpdir} = require('node:os');
const {join, resolve} = require('node:path');
const {pathToFileURL} = require('node:url');
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
test('native contracts validated without effects; incomplete untitled files and UTF16 diagnostics',async t=> {
    const s=await session(t);
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const uri=pathToFileURL(join(dir,'lang.yl')).href;
    s.open(uri,'import { lexicalScope } from "std/variables"\nnode Root = "x" { meanings { lexicalScope() } }\nentry Root');
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
    const many=Array.from({length:4000},(_,i)=>`pattern P${i}="x"`).join('\n')+'\nnode Root="x"\nentry Root';
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
    const text=`import * as types from "std/types"
import { children, first } from "std/query"
symbol number
operation annotateWithType(value) { types::annotate(self,value) }
operation restrictTypeTo(value) { types::restrictTo(self,value) }
operation typeof(value) = types::typeof(value)
node Value="x" { meanings { annotateWithType(number) } }
node Root=value: Value { meanings {
    annotateWithType(typeof(value))
    restrictTypeTo(number)
    types::annotate(value,[number])
    types::annotate(first(children(self)),number)
    types::annotate("invalid-key",number)
    match (types::typeof(self)) {
        number => types::annotate(self,number),
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
    assert.match(edits[0].newText,/from "std\/query"/);
    assert.match(edits[0].newText,/operation annotateWithType\(value\)/);
    assert.match(edits[0].newText,/types::restrictTo/);
    assert.match(edits[0].newText,/types::typeof/);
    s.change(uri,text.replace('types::annotate(self,value)','types::annotate(self)'),2);
    assert.ok((await s.diagnostic(uri,2)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replace('types::restrictTo(self,value)','types::restrictTo(self)'),3);
    assert.ok((await s.diagnostic(uri,3)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replace('first(children(self))','first()'),4);
    assert.ok((await s.diagnostic(uri,4)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text.replaceAll('types::annotate','types::ensure'),5);
    assert.ok((await s.diagnostic(uri,5)).diagnostics.some(d=>d.severity===1));
    s.change(uri,text,6);
    assert.deepEqual((await s.diagnostic(uri,6)).diagnostics,[]);
});

test('imported child CodeLens uses entry trivia and buffer changes preserve spaces',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-child-trivia-'));t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'),child=join(dir,'let.yl');
    await writeFile(entry,'import { Let } from "./let"\ntrivia Space = / +/\nnode Root = Let\nentry Root { trivia Space }');
    const text='pattern Name=/[a-z]+/ { example "name" }\nexport node Let = "let" Name "=" "42" ";"';
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
    const text='node Root = "x" { meanings { match (a,b) { number,number => type(number), string,string | string,number | number,string => type(string), _ => {} } } }';
    s.open(uri,text);await s.diagnostic(uri,1);
    const [edit]=await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}});
    const lines=edit.newText.split('\n').filter(line=>line.includes('=>'));
    assert.equal(lines.length,3);assert.equal(new Set(lines.map(line=>line.indexOf('=>'))).size,1);
});
