import { execFile } from 'node:child_process';
import { isAbsolute, dirname, resolve } from 'node:path';
import type { Cancellation } from './formatter';
export interface ExamplePreview { definition: string; span: { file: string; start: number; end: number }; text: string | null; reason: string | null }
/** Syntax-only compiler previews. No shell and no semantic library execution. */
export async function loadPreviews(executable: string, entry: string, token: Cancellation): Promise<ExamplePreview[]> {
    if (!isAbsolute(entry) || !executable.trim() || (!isAbsolute(executable) && /[\\/]/.test(executable))) throw new Error('Use an absolute exampleEntry and a CLI executable name or absolute path.');
    if (token.isCancellationRequested) return [];
    return new Promise((done, reject) => {
        const child = execFile(executable, ['examples', entry], { cwd: dirname(entry), windowsHide: true, timeout: 30000, maxBuffer: 1024 * 1024, env: { ...process.env, NO_COLOR: '1' } }, (error, stdout, stderr) => {
            subscription.dispose();
            if (token.isCancellationRequested) done([]);
            else if (error) reject(new Error(stderr.trim() || error.message));
            else {
                try {
                    const result: unknown = JSON.parse(stdout);
                    if (!Array.isArray(result) || !result.every(validPreview)) throw new Error('Invalid YL example output');
                    done(result);
                } catch (error) { reject(error); }
            }
        });
        const subscription = token.onCancellationRequested(() => { child.kill(); });
        if (token.isCancellationRequested) child.kill();
    });
}
function validPreview(value: unknown): value is ExamplePreview {
    if (!value || typeof value !== 'object') return false;
    const p = value as Partial<ExamplePreview>;
    return typeof p.definition === 'string' && (p.text === null || typeof p.text === 'string') && !!p.span && typeof p.span.file === 'string' && Number.isSafeInteger(p.span.start) && Number.isSafeInteger(p.span.end) && p.span.start >= 0 && p.span.end >= p.span.start;
}
export function documentPreviews(previews: ExamplePreview[], entry: string, document: string, length: number): ExamplePreview[] {
    return previews.filter(p => p.text !== null && resolve(dirname(entry), p.span.file) === resolve(document) && p.span.end <= length);
}

export function byteOffsetToPosition(source: string, offset: number): number {
    return Buffer.from(source, 'utf8').subarray(0, offset).toString('utf8').length;
}
