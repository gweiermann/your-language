const {test} = require('node:test');
const assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const {mkdtemp, writeFile, rm} = require('node:fs/promises');
const {tmpdir} = require('node:os');
const {join, resolve} = require('node:path');
const {pathToFileURL} = require('node:url');
const rpc = require('vscode-jsonrpc/node');
const executable = process.env.YL_TEST_CLI || resolve(__dirname, '../../../target/debug/yl' + (process.platform === 'win32' ? '.exe' : ''));
async function session(t, entry) {
    const child=spawn(executable,['lsp'],{windowsHide:true});
    let stderr=''; child.stderr.on('data',chunk=>stderr+=chunk);
    const connection=rpc.createMessageConnection(new rpc.StreamMessageReader(child.stdout),new rpc.StreamMessageWriter(child.stdin));
    const notices=[]; connection.onNotification('textDocument/publishDiagnostics',p=>notices.push(p)); connection.listen();
    t.after(async()=> { try { await connection.sendRequest('shutdown'); await connection.sendNotification('exit'); await Promise.race([new Promise(resolve=>child.once('close',resolve)),new Promise((_,reject)=>{const timer=setTimeout(()=>reject(Error('Server did not exit with stdin open')),3000);timer.unref();})]); } finally { connection.dispose(); child.kill(); } });
    const init=await connection.sendRequest('initialize',{processId:process.pid,capabilities:{},initializationOptions:{entry}});
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
    return {connection,open,change,diagnostic,hints,notices};
}
test('real LSP client: unsaved imports, hints, diagnostics, formatting and close fallback',async t=> {
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const entry=join(dir,'language.yl'), helper=join(dir,'name.yl');
    await writeFile(helper,'export pattern Name = "disk"');
    const root=pathToFileURL(entry).href, dep=pathToFileURL(helper).href.replace(/^file:\/\/\/([A-Z]):/,(_,drive)=>`file:///${drive.toLowerCase()}:`);
    const s=await session(t,entry);
    s.open(root,'import { Name } from "./name"\nnode Root = Name\nentry Root'); await s.diagnostic(root,1);
    assert.match(JSON.stringify(await s.hints(root)),/disk/);
    s.open(dep,'// 😀\nexport pattern Name = "buffer"'); await s.diagnostic(dep,1);
    assert.match(JSON.stringify(await s.hints(root)),/buffer/);
    s.change(dep,'// 😀\nexport pattern Name = @',2);
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
    s.open('untitled:bad','node Root = "😀" @');
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


test('identity typing namespaces and authored wrappers validate and format without executing effects',async t=> {
    const s=await session(t);
    const dir=await mkdtemp(join(tmpdir(),'yl-lsp-types-')); t.after(()=>rm(dir,{recursive:true,force:true}));
    const uri=pathToFileURL(join(dir,'lang.yl')).href;
    const text=`import * as types from "std/types"
import * as variables from "std/variables"
symbol number
operation annotateWithType(value) { types::ensure(variables::identity(),value) }
node Root="x" { meanings { annotateWithType(number) types::ensure("invalid-key",number) } }
entry Root`;
    s.open(uri,text);
    assert.deepEqual((await s.diagnostic(uri,1)).diagnostics,[]);
    assert.match(JSON.stringify(await s.hints(uri)),/example/);
    const edits=await s.connection.sendRequest('textDocument/formatting',{textDocument:{uri},options:{tabSize:4,insertSpaces:true}});
    assert.match(edits[0].newText,/import \* as types/);
    assert.match(edits[0].newText,/operation annotateWithType\(value\)/);
    s.change(uri,text.replace('types::ensure(variables::identity(),value)','types::ensure(variables::identity())'),2);
    assert.ok((await s.diagnostic(uri,2)).diagnostics.some(d=>d.severity===1));
});
