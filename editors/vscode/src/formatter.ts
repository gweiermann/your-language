import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export interface Cancellation {
    readonly isCancellationRequested: boolean;
    onCancellationRequested(listener: () => void): { dispose(): void };
}

export class FormattingCancelled extends Error {}

/** Format buffer text, never the user's file. No shell or workspace code is used. */
export async function formatBuffer(source: string, executable: string, token: Cancellation): Promise<string> {
    if (token.isCancellationRequested) throw new FormattingCancelled();
    const directory = await mkdtemp(join(tmpdir(), 'your-language-format-'));
    const file = join(directory, 'buffer.yl');
    try {
        await writeFile(file, source, 'utf8');
        if (token.isCancellationRequested) throw new FormattingCancelled();
        await new Promise<void>((resolve, reject) => {
            const child = execFile(executable, ['fmt', '--', file], {
                cwd: directory, windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024,
                env: { ...process.env, NO_COLOR: '1' },
            }, (error, _stdout, stderr) => {
                subscription.dispose();
                if (token.isCancellationRequested) reject(new FormattingCancelled());
                else if (error) {
                    const detail = stderr.trim() || error.message;
                    reject(new Error(`YL formatting failed: ${detail.replaceAll(file, 'current document')}\nCheck yourLanguage.cliPath and use a CLI that supports yl fmt.`));
                } else resolve();
            });
            const subscription = token.onCancellationRequested(() => { child.kill(); });
            if (token.isCancellationRequested) child.kill();
        });
        if (token.isCancellationRequested) throw new FormattingCancelled();
        return await readFile(file, 'utf8');
    } finally {
        await rm(directory, { recursive: true, force: true });
    }
}
