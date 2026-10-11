const { test } = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
function load(t, trusted) {
    const clients=[]; let grant; let changed;
    const vscode={workspace:{isTrusted:trusted,getConfiguration:()=>({get:(key,fallback)=>fallback}),createFileSystemWatcher:()=>({dispose(){}}),onDidGrantWorkspaceTrust:fn=>{grant=fn;return {dispose(){}}},onDidChangeConfiguration:fn=>{changed=fn;return {dispose(){}}}},window:{createOutputChannel:()=>({appendLine(){},dispose(){}})}};
    class Client { constructor(id,name,server,options){this.server=server;this.options=options;this.starts=0;this.notifications=[];clients.push(this);} async start(){this.starts++;if(Client.fail)throw Error('missing executable');} async stop(){} async sendNotification(...args){this.notifications.push(args);} }
    const original=Module._load;
    Module._load=function(id,...args){ if(id==='vscode')return vscode; if(id==='vscode-languageclient/node')return {LanguageClient:Client}; return original.call(this,id,...args); };
    const path=require.resolve('../out/extension'); delete require.cache[path]; const extension=require(path); Module._load=original;
    t.after(async()=>{await extension.deactivate();delete require.cache[path];});
    return {extension,vscode,clients,Client,grant:()=>grant(),change:(setting='yourLanguage.exampleEntry')=>changed({affectsConfiguration:key=>key===setting})};
}
test('one LSP client uses stdio, full document selectors and file dependency watcher',async t=>{const e=load(t,true);await e.extension.activate({subscriptions:[]}); assert.equal(e.clients.length,1);const c=e.clients[0];assert.deepEqual(c.server,{command:'yl',args:['lsp'],options:{shell:false}});assert.equal(c.options.documentSelector.length,2);assert.equal(c.starts,1);e.change();assert.equal(c.notifications[0][0],'workspace/didChangeConfiguration');});
test('untrusted workspace does not launch server; trust grant starts it once',async t=>{const e=load(t,false);await e.extension.activate({subscriptions:[]});assert.equal(e.clients.length,0);e.vscode.workspace.isTrusted=true;e.grant();await new Promise(r=>setImmediate(r));assert.equal(e.clients.length,1);e.grant();assert.equal(e.clients.length,1);});
test('fixing executable configuration retries after failed startup',async t=>{const e=load(t,true);e.Client.fail=true;await e.extension.activate({subscriptions:[]});assert.equal(e.clients.length,1);e.Client.fail=false;e.change('yourLanguage.cliPath');await new Promise(r=>setImmediate(r));assert.equal(e.clients.length,2);assert.equal(e.clients[1].starts,1);});
