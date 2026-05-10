//! Span 操作ユーティリティ
//!
//! analyzer 内部では `Range<usize>` (バイトオフセット) で span を扱う．
//! AST 由来の `chumsky::span::SimpleSpan` から `Range<usize>` への変換と，
//! offset を span に含むかを判定する小さな関数群を提供する．

use chumsky::span::SimpleSpan;

pub use fsl_parser::Span;

/// `SimpleSpan` を `Range<usize>` に正規化する
pub fn to_range(s: SimpleSpan) -> Span {
    s.start..s.end
}

/// 半開区間 `[start, end)` で offset を内包するか判定する
pub fn contains(span: &Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

/// 識別子上の hover/Goto 用に終端も含めて判定する閉区間版
pub fn contains_inclusive(span: &Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
