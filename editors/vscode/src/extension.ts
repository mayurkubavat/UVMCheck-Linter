import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('uvmcheck');
    if (!config.get<boolean>('enable', true)) {
        return;
    }

    const executablePath = resolveExecutable(context, config.get<string>('executablePath', 'uvmcheck'));
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

    const serverExecutable: Executable = {
        command: executablePath,
        args: ['--lsp'],
        options: {
            env: process.env,
            // The server finds .uvmcheck/config.py from the workspace root.
            cwd: workspaceRoot
        }
    };

    const serverOptions: ServerOptions = {
        run: serverExecutable,
        debug: serverExecutable
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'systemverilog' },
            { scheme: 'file', language: 'verilog' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{sv,svh,v}')
        }
    };

    client = new LanguageClient(
        'uvmcheck-lsp',
        'UVMCheck Language Server',
        serverOptions,
        clientOptions
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('uvmcheck.restartServer', async () => {
            if (client) {
                await client.stop();
                client.start();
                vscode.window.showInformationMessage('UVMCheck language server restarted.');
            }
        })
    );

    client.start();
}

/**
 * The binary to run: an explicit path from the settings, else the one bundled
 * with the extension for this platform (bin/<platform>-<arch>/uvmcheck), else
 * `uvmcheck` from PATH.
 */
function resolveExecutable(context: vscode.ExtensionContext, configured: string): string {
    if (configured && configured !== 'uvmcheck') {
        return configured;
    }
    const exe = process.platform === 'win32' ? 'uvmcheck.exe' : 'uvmcheck';
    const bundled = context.asAbsolutePath(path.join('bin', `${process.platform}-${process.arch}`, exe));
    try {
        fs.accessSync(bundled, fs.constants.X_OK);
        return bundled;
    } catch {
        return 'uvmcheck';
    }
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
