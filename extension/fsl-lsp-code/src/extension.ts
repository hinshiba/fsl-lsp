import { commands, window, ExtensionContext } from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

/** 単一のモジュール内でのクライアント */
let client: LanguageClient | undefined;

/**
 * 新しいクライアントを作成する．
 * @returns 作成されたクライアント．失敗した場合は`undefined`
 */
function newLanguageClient(): LanguageClient | undefined {
    const serverOptions: ServerOptions = { command: "D:\\dev\\_univ\\g3t1\\exA_tool\\fsl-lsp\\target\\debug\\fsl-ls.exe" };
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
 * @param client クライアント
 */
async function restartLanguageServer(): Promise<void> {
    if (client === undefined) {
        client = newLanguageClient();
        if (client === undefined) {
            throw new Error("Failed to create language client");
        }
        await client.start();
    } else {
        await client.restart();
    }
}

export async function activate(context: ExtensionContext) {
    await restartLanguageServer();
    context.subscriptions.push(
        commands.registerCommand("fsl.restartLanguageServer", restartLanguageServer),
        { dispose: () => client?.stop() }
    );
}

export async function deactivate() { }
