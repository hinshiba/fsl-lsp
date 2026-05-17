//! ワークスペース走査とモジュール索引構築
//!
//! 開いていないファイルも含めワークスペース配下の `.fsl` を集め，
//! `extends` 継承と `instance.member` 補完のための横断索引を作る．

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fsl_analyzer::ModuleIndex;
use tower_lsp::lsp_types::{InitializeParams, Url};

/// 走査対象とするワークスペースルート群を `InitializeParams` から取り出す
///
/// `workspace_folders` を優先し，無ければ非推奨の `root_uri` を用いる．
pub fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(folders) = &params.workspace_folders {
        for f in folders {
            if let Ok(p) = f.uri.to_file_path() {
                roots.push(p);
            }
        }
    }

    // フォルダ指定が無い場合のフォールバック
    if roots.is_empty() {
        #[allow(deprecated)]
        if let Some(uri) = &params.root_uri {
            if let Ok(p) = uri.to_file_path() {
                roots.push(p);
            }
        }
    }

    roots
}

/// `root` 配下を再帰走査し `.fsl` ファイルパスを集める
pub fn scan_fsl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(scan_fsl_files(&p));
        } else if p.extension().and_then(|s| s.to_str()) == Some("fsl") {
            out.push(p);
        }
    }
    out
}

/// 全ソースからモジュール横断索引を構築する
pub fn build_index(sources: &HashMap<Url, String>) -> ModuleIndex {
    let mut index = ModuleIndex::default();
    for text in sources.values() {
        index.add_source(text);
    }
    index
}
