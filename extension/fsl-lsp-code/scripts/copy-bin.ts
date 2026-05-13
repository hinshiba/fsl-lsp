import { fileURLToPath } from "node:url";
import path from "node:path";
import fs, { copyFileSync } from "node:fs"

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.join(__dirname, "../../../");
const lsBinRoot = path.join(workspaceRoot, "target");

const extensionRoot = path.join(__dirname, "../");
const extensionBinRoot = path.join(extensionRoot, "bin");

type CopyEntry = { target: string, fromPath: string; toPath: string };

const entries: CopyEntry[] = [
    { target: "win", fromPath: path.join(lsBinRoot, "debug/fsl-ls.exe"), toPath: path.join(extensionBinRoot, "win/fsl-ls.exe") },
    { target: "linux", fromPath: path.join(lsBinRoot, "x86_64-unknown-linux-gnu/debug/fsl-ls"), toPath: path.join(extensionBinRoot, "linux/fsl-ls") },
];


entries.forEach(entry => {
    const isExist = fs.existsSync(entry.fromPath);
    if (isExist) {
        console.log("[copy] %s", entry.target)
        // fs.copyFileSyncは親フォルダがないとエラーなので
        fs.mkdirSync(path.dirname(entry.toPath), { recursive: true });
        fs.copyFileSync(entry.fromPath, entry.toPath);
    } else {
        console.log("[skip]: %s because missing: %s", entry.target, entry.fromPath)
    }
});


