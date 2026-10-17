import * as vscode from 'vscode';
import { isAbsolute } from 'node:path';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const output = vscode.window.createOutputChannel('Your Language');
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.yl');
    const projects = vscode.workspace.createFileSystemWatcher('**/yl.json');
    const originalLanguages = new Map<string, string>();
    let generation = 0;
    context.subscriptions.push(output, watcher, projects);
    async function associate(document: vscode.TextDocument): Promise<void> {
        if (!client || !vscode.workspace.isTrusted || document.uri.scheme !== 'file' || document.languageId === 'yl' || document.uri.path.endsWith('/yl.json')) return;
        const activeClient = client;
        const currentGeneration = generation;
        try {
            const project = await activeClient.sendRequest<{ matches: boolean }>('yl/project', { textDocument: { uri: document.uri.toString() } });
            if (client !== activeClient || generation !== currentGeneration || document.isClosed) return;
            const key = document.uri.toString();
            if (project.matches && document.languageId !== 'yl-program') {
                originalLanguages.set(key, document.languageId);
                await vscode.languages.setTextDocumentLanguage(document, 'yl-program');
            } else if (!project.matches && document.languageId === 'yl-program') {
                const previous = originalLanguages.get(key);
                originalLanguages.delete(key);
                if (previous) await vscode.languages.setTextDocumentLanguage(document, previous);
            }
        } catch (error) { output.appendLine(`Project discovery: ${String(error)}`); }
    }
    async function associateOpenDocuments(): Promise<void> {
        generation++;
        await Promise.all(vscode.workspace.textDocuments.map(associate));
    }
    const isConfig = (document: vscode.TextDocument): boolean => document.uri.scheme === 'file' && document.uri.path.endsWith('/yl.json');
    async function openConfig(document: vscode.TextDocument): Promise<void> {
        await client?.sendNotification('textDocument/didOpen', { textDocument: { uri: document.uri.toString(), languageId: 'json', version: document.version, text: document.getText() } });
    }
    context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(document => {
        if (isConfig(document)) void openConfig(document).then(associateOpenDocuments);
        else void associate(document);
    }));
    context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(event => {
        if (isConfig(event.document)) void client?.sendNotification('textDocument/didChange', {
            textDocument: { uri: event.document.uri.toString(), version: event.document.version }, contentChanges: [{ text: event.document.getText() }],
        }).then(associateOpenDocuments);
    }));
    context.subscriptions.push(vscode.workspace.onDidCloseTextDocument(document => {
        if (isConfig(document)) void client?.sendNotification('textDocument/didClose', { textDocument: { uri: document.uri.toString() } }).then(associateOpenDocuments);
    }));
    for (const event of [projects.onDidCreate, projects.onDidChange, projects.onDidDelete]) {
        context.subscriptions.push(event(() => { void associateOpenDocuments(); }));
    }
    context.subscriptions.push(vscode.commands.registerCommand('yourLanguage.copyExample', async (text: unknown) => {
        if (typeof text === 'string') await vscode.env.clipboard.writeText(text);
    }));
    async function start(): Promise<void> {
        if (!vscode.workspace.isTrusted || client) return;
        const config = vscode.workspace.getConfiguration('yourLanguage');
        const command = config.get<string>('cliPath', 'yl');
        if (!command.trim() || (!isAbsolute(command) && /[\\/]/.test(command))) {
            output.appendLine('Set yourLanguage.cliPath to an absolute executable path or a name on PATH.');
            return;
        }
        const server: ServerOptions = { command, args: ['lsp'], options: { shell: false } };
        const options: LanguageClientOptions = {
            documentSelector: [{ language: 'yl', scheme: 'file' }, { language: 'yl', scheme: 'untitled' }, { language: 'yl-program', scheme: 'file' }],
            outputChannel: output,
            initializationOptions: { entry: config.get<string>('exampleEntry', ''), exampleDisplay: config.get<string>('exampleDisplay', 'above') },
            synchronize: { fileEvents: [watcher, projects] },
        };
        client = new LanguageClient('yourLanguage', 'Your Language', server, options);
        try {
            await client.start();
            await Promise.all(vscode.workspace.textDocuments.filter(isConfig).map(openConfig));
            await associateOpenDocuments();
        }
        catch (error) { output.appendLine(`Language server failed: ${String(error)}`); client = undefined; }
    }
    context.subscriptions.push(vscode.workspace.onDidGrantWorkspaceTrust(() => { void start(); }));
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(event => {
        if (event.affectsConfiguration('yourLanguage.exampleEntry') || event.affectsConfiguration('yourLanguage.exampleDisplay')) {
            void client?.sendNotification('workspace/didChangeConfiguration', {
                settings: { entry: vscode.workspace.getConfiguration('yourLanguage').get<string>('exampleEntry', ''),
                    exampleDisplay: vscode.workspace.getConfiguration('yourLanguage').get<string>('exampleDisplay', 'above') },
            });
        }
        if (event.affectsConfiguration('yourLanguage.cliPath')) {
            const previous = client; client = undefined;
            void (async () => { await previous?.stop(); await start(); })();
        }
    }));
    await start();
}

export async function deactivate(): Promise<void> {
    const previous = client; client = undefined;
    await previous?.stop();
}
