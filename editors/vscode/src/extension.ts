import * as vscode from 'vscode';
import { isAbsolute } from 'node:path';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const output = vscode.window.createOutputChannel('Your Language');
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.yl');
    context.subscriptions.push(output, watcher);
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
            documentSelector: [{ language: 'yl', scheme: 'file' }, { language: 'yl', scheme: 'untitled' }],
            outputChannel: output,
            initializationOptions: { entry: config.get<string>('exampleEntry', ''), exampleDisplay: config.get<string>('exampleDisplay', 'above') },
            synchronize: { fileEvents: watcher },
        };
        client = new LanguageClient('yourLanguage', 'Your Language', server, options);
        try { await client.start(); }
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
