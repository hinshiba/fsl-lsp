//! 位置変換ユーティリティ
//!
//! LSP の `Position` (UTF-16 列ベースが標準) と byte offset の相互変換．
//! 既存の `offset_to_position` 同様，本実装は UTF-8 桁を character として扱う簡易版．

use line_index::{LineCol, LineIndex, TextSize};
use tower_lsp::lsp_types::Position;

/// `Position` を byte offset に変換する
pub fn position_to_offset(li: &LineIndex, p: Position) -> Option<usize> {
    let lc = LineCol {
        line: p.line,
        col: p.character,
    };
    let off: TextSize = li.offset(lc)?;
    Some(u32::from(off) as usize)
}
