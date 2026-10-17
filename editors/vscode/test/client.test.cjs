const { test } = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
test('project configuration activates the extension before any YL definition is opened',()=> {
    const manifest=require('../package.json');
    assert.ok(manifest.activationEvents.includes('workspaceContains:**/yl.json'));
    assert.ok(manifest.contributes.languages.some(language=>language.id==='yl-program'&&!language.extensions));
});
function load(t, trusted) {
    const clients=[]; const commands=new Map(), clipboard=[]; let grant; let changed; let opened; let edited; let closed; const watchers=[]; const associations=[];
    const vscode={languages:{setTextDocumentLanguage:async(document,id)=>{associations.push(id);document.languageId=id;return document;}},commands:{registerCommand:(name,fn)=>{commands.set(name,fn);return {dispose(){}}}},env:{clipboard:{writeText:async text=>clipboard.push(text)}},workspace:{isTrusted:trusted,getConfiguration:()=>({get:(key,fallback)=>fallback}),textDocuments:[],createFileSystemWatcher:()=>{const watcher={dispose(){},onDidCreate(fn){watcher.created=fn;return {dispose(){}}},onDidChange(fn){watcher.changed=fn;return {dispose(){}}},onDidDelete(fn){watcher.deleted=fn;return {dispose(){}}}};watchers.push(watcher);return watcher;},onDidOpenTextDocument:fn=>{opened=fn;return {dispose(){}}},onDidChangeTextDocument:fn=>{edited=fn;return {dispose(){}}},onDidCloseTextDocument:fn=>{closed=fn;return {dispose(){}}},onDidGrantWorkspaceTrust:fn=>{grant=fn;return {dispose(){}}},onDidChangeConfiguration:fn=>{changed=fn;return {dispose(){}}}},window:{createOutputChannel:()=>({appendLine(){},dispose(){}})}};
    class Client { constructor(id,name,server,options){this.server=server;this.options=options;this.starts=0;this.notifications=[];clients.push(this);} async start(){this.starts++;if(Client.fail)throw Error('missing executable');} async stop(){} async sendNotification(...args){this.notifications.push(args);} async sendRequest(method,args){return {matches:Client.matches??false};} }
    const original=Module._load;
    Module._load=function(id,...args){ if(id==='vscode')return vscode; if(id==='vscode-languageclient/node')return {LanguageClient:Client}; return original.call(this,id,...args); };
    const path=require.resolve('../out/extension'); delete require.cache[path]; const extension=require(path); Module._load=original;
    t.after(async()=>{await extension.deactivate();delete require.cache[path];});
    return {extension,vscode,clients,Client,commands,clipboard,watchers,associations,open:document=>opened(document),edit:document=>edited({document}),close:document=>closed(document),grant:()=>grant(),change:(setting='yourLanguage.exampleEntry')=>changed({affectsConfiguration:key=>key===setting})};
}
test('one LSP client uses stdio, full document selectors and file dependency watcher',async t=>{const e=load(t,true);await e.extension.activate({subscriptions:[]}); assert.equal(e.clients.length,1);const c=e.clients[0];assert.deepEqual(c.server,{command:'yl',args:['lsp'],options:{shell:false}});assert.equal(c.options.documentSelector.length,3);assert.equal(c.starts,1);e.change();assert.equal(c.notifications[0][0],'workspace/didChangeConfiguration');});
test('untrusted workspace does not launch server; trust grant starts it once',async t=>{const e=load(t,false);await e.extension.activate({subscriptions:[]});assert.equal(e.clients.length,0);e.vscode.workspace.isTrusted=true;e.grant();await new Promise(r=>setImmediate(r));assert.equal(e.clients.length,1);e.grant();assert.equal(e.clients.length,1);});
test('fixing executable configuration retries after failed startup',async t=>{const e=load(t,true);e.Client.fail=true;await e.extension.activate({subscriptions:[]});assert.equal(e.clients.length,1);e.Client.fail=false;e.change('yourLanguage.cliPath');await new Promise(r=>setImmediate(r));assert.equal(e.clients.length,2);assert.equal(e.clients[1].starts,1);});

test('copy example command preserves complete source and ignores invalid arguments',async t=> {
    const e=load(t,true); await e.extension.activate({subscriptions:[]});
    assert.equal(e.clients[0].options.initializationOptions.exampleDisplay,'above');
    const copy=e.commands.get('yourLanguage.copyExample');
    await copy('let x="quoted";\nnext();'); await copy({uri:'not a source string'});
    assert.deepEqual(e.clipboard,['let x="quoted";\nnext();']);
    e.change('yourLanguage.exampleDisplay'); assert.equal(e.clients[0].notifications.length,1);
    assert.equal(e.clients[0].notifications[0][1].settings.exampleDisplay,'above');
});


test('only configured program documents change language, and removed configurations restore it',async t=> {
    const e=load(t,true);await e.extension.activate({subscriptions:[]});
    const document={uri:{scheme:'file',path:'/project/program.ts',toString:()=> 'file:///project/program.ts'},languageId:'typescript',isClosed:false};
    e.vscode.workspace.textDocuments.push(document);
    e.open(document);await new Promise(r=>setImmediate(r));assert.deepEqual(e.associations,[]);
    e.Client.matches=true;e.open(document);await new Promise(r=>setImmediate(r));assert.deepEqual(e.associations,['yl-program']);
    e.Client.matches=false;e.close({uri:{scheme:'file',path:'/project/yl.json',toString:()=> 'file:///project/yl.json'}});await new Promise(r=>setImmediate(r));assert.deepEqual(e.associations,['yl-program','typescript']);
    e.Client.matches=true;e.open(document);await new Promise(r=>setImmediate(r));
    e.Client.matches=false;e.watchers[1].deleted();await new Promise(r=>setImmediate(r));assert.equal(document.languageId,'typescript');
});


test('configuration buffers synchronize without claiming JSON formatting or highlighting',async t=> {
    const e=load(t,true);await e.extension.activate({subscriptions:[]});
    const config={uri:{scheme:'file',path:'/project/yl.json',toString:()=> 'file:///project/yl.json'},version:1,getText:()=>'{"entry":"language.yl","files":["*.ts"]}'};
    e.open(config);await new Promise(r=>setImmediate(r));
    e.edit(config);await new Promise(r=>setImmediate(r));
    e.close(config);await new Promise(r=>setImmediate(r));
    assert.deepEqual(e.clients[0].notifications.map(n=>n[0]),['textDocument/didOpen','textDocument/didChange','textDocument/didClose']);
    assert.ok(e.clients[0].options.documentSelector.every(selector=>selector.language!=='json'&&!selector.pattern));
});
