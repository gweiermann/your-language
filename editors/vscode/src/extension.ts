import * as vscode from 'vscode';
import { loadPreviews, documentPreviews, byteOffsetToPosition } from './examples';
import { isAbsolute } from 'node:path';
import { formatBuffer, FormattingCancelled } from './formatter';

export function activate(context: vscode.ExtensionContext): void {
    const output = vscode.window.createOutputChannel('Your Language');
    context.subscriptions.push(output, vscode.languages.registerDocumentFormattingEditProvider(
        [{ language: 'yl', scheme: 'file' }, { language: 'yl', scheme: 'untitled' }],
        {
            async provideDocumentFormattingEdits(document, _options, token) {
                if (!vscode.workspace.isTrusted) {
                    void vscode.window.showWarningMessage('Trust this workspace to enable YL CLI formatting.');
                    return [];
                }
                const version = document.version;
                const source = document.getText();
                const executable = vscode.workspace.getConfiguration('yourLanguage', document.uri).get<string>('cliPath', 'yl');
                try {
                    if (!executable.trim() || (!isAbsolute(executable) && /[\\/]/.test(executable))) {
                        throw new Error('Set yourLanguage.cliPath to an absolute executable path or an executable name on PATH.');
                    }
                    const formatted = await formatBuffer(source, executable, token);
                    if (token.isCancellationRequested || document.isClosed || document.version !== version || formatted === source) return [];
                    return [vscode.TextEdit.replace(new vscode.Range(document.positionAt(0), document.positionAt(source.length)), formatted)];
                } catch (error) {
                    if (error instanceof FormattingCancelled || token.isCancellationRequested) return [];
                    const message = error instanceof Error ? error.message : String(error);
                    output.appendLine(message);
                    void vscode.window.showErrorMessage('YL formatting failed. Your document was not changed.', 'Show Details').then(action => {
                        if (action === 'Show Details') output.show();
                    });
                    return [];
                }
            },
        },
    ));
    context.subscriptions.push(vscode.languages.registerInlayHintsProvider({ language: 'yl', scheme: 'file' }, {
        async provideInlayHints(document, range, token) {
            const config = vscode.workspace.getConfiguration('yourLanguage', document.uri);
            const entry = config.get<string>('exampleEntry', '');
            if (!entry || !vscode.workspace.isTrusted || document.isDirty || token.isCancellationRequested) return [];
            const version = document.version;
            try {
                const previews = await loadPreviews(config.get<string>('cliPath', 'yl'), entry, token);
                if (document.isClosed || document.isDirty || document.version !== version || token.isCancellationRequested) return [];
                return documentPreviews(previews, entry, document.uri.fsPath, Buffer.byteLength(document.getText(), 'utf8')).flatMap(preview => {
                    const position = document.positionAt(byteOffsetToPosition(document.getText(), preview.span.end));
                    if (!range.contains(position)) return [];
                    const hint = new vscode.InlayHint(position, ` example: ${JSON.stringify(preview.text)}`, vscode.InlayHintKind.Type);
                    hint.paddingLeft = true;
                    hint.tooltip = 'Experimental parser-validated syntax example. Optional parts are omitted; * and + contain one item.';
                    return [hint];
                });
            } catch (error) {
                output.appendLine(`Example preview unavailable: ${error instanceof Error ? error.message : String(error)}`);
                return [];
            }
        },
    }));

}
