import { commands, window, ExtensionContext } from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";
import * as path from "path";

/** 単一のモジュール内でのクライアント */
let client: LanguageClient | undefined;

/** 拡張機能ルートからのサーバ実行ファイル相対パス（プラットフォーム別） */
const binRelPath: { [key: string]: string } = {
    "win32": path.join("bin", "win", "fsl-ls.exe"),
    "linux": path.join("bin", "linux", "fsl-ls"),
};

/**
 * 新しいクライアントを作成する．
 * @param serverPath サーバ実行ファイルの絶対パス
 * @returns 作成されたクライアント．失敗した場合は`undefined`
 */
function newLanguageClient(serverPath: string): LanguageClient | undefined {
    const serverOptions: ServerOptions = { command: serverPath };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            {
                scheme: "file",
                language: "fsl",
            }
        ]
    };
    try {
        return new LanguageClient("fsl-ls", serverOptions, clientOptions);
    } catch (e) {
        window.showErrorMessage("Failed to start fsl language server.");
        window.showErrorMessage(`${e}`);
    }
}

/**
 * 言語サーバーを再起動する．起動していない場合は起動する．
 * @param serverPath サーバ実行ファイルの絶対パス
 */
async function restartLanguageServer(serverPath: string): Promise<void> {
    if (client === undefined) {
        client = newLanguageClient(serverPath);
        if (client === undefined) {
            throw new Error("Failed to create language client");
        }
        await client.start();
    } else {
        await client.restart();
    }
}

export async function activate(context: ExtensionContext) {
    const relPath = binRelPath[process.platform];
    if (relPath === undefined) {
        window.showErrorMessage(`Unsupported platform: ${process.platform}`);
        return;
    }
    const serverPath = context.asAbsolutePath(relPath);

    await restartLanguageServer(serverPath);
    context.subscriptions.push(
        commands.registerCommand("fsl.restartLanguageServer", () => restartLanguageServer(serverPath)),
        { dispose: () => client?.stop() }
    );
}

export async function deactivate() { }
