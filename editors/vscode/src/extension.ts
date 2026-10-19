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

    const executablePath = config.get<string>('executablePath', 'uvmcheck');

    const serverExecutable: Executable = {
        command: executablePath,
        args: ['--lsp'],
        options: {
            env: process.env
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

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
