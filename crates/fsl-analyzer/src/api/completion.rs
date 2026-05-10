//! Completion
//!
//! 指定 offset から見えるシンボル群と，
//! キーワード / 組込み型 / ビルトイン関数のスタティックリストを返す．

use crate::symbol::Symbol;
use crate::AnalysisResult;

/// FSL のキーワード一覧
pub const KEYWORDS: &[&str] = &[
    "module", "trait", "def", "val", "reg", "mem", "input", "output", "always", "initial",
    "stage", "state", "par", "seq", "any", "alt", "if", "else", "match", "case", "generate",
    "relay", "finish", "goto", "new", "extends", "with", "true", "false", "private", "type",
];

/// 組込み型名
pub const BUILTIN_TYPES: &[&str] = &["Bit", "Boolean", "Int", "Unit", "String", "Array", "List"];

/// completion 候補のまとまり
pub struct CompletionList<'a> {
    pub symbols: Vec<&'a Symbol>,
    pub keywords: &'static [&'static str],
    pub builtin_types: &'static [&'static str],
    pub builtins: &'static [&'static str],
}

/// `offset` における completion 候補を返す
pub fn completions_at<'a>(result: &'a AnalysisResult, offset: usize) -> CompletionList<'a> {
    CompletionList {
        symbols: result.symbols.visible_at(offset),
        keywords: KEYWORDS,
        builtin_types: BUILTIN_TYPES,
        builtins: crate::builtin::builtins().all(),
    }
}
