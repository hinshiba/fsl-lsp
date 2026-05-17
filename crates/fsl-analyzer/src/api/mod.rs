//! LSP 機能向け公開 API
//!
//! `definition_at` / `hover_at` / `completions_at` の三本立て．
//! いずれも `AnalysisResult` と `offset` を受け取り
//! 検索結果を LSP 中立な形式で返す．

pub mod completion;
pub mod definition;
pub mod hover;

pub use completion::{
    completions_at, member_completions, CompletionList, BUILTIN_TYPES, KEYWORDS,
};
pub use definition::definition_at;
pub use hover::{hover_at, HoverPayload};
