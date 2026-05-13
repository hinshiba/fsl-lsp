//! Goto Definition
//!
//! `offset` 上にある参照の解決結果から定義位置の span を取り出す．

use crate::span::Span;
use crate::symbols::ResolvedTo;
use crate::AnalysisResult;

/// `offset` 上の参照を解決し，定義の `def_span` を返す
/// 参照が無い・ビルトイン・未解決のいずれの場合も `None`
pub fn definition_at(result: &AnalysisResult, offset: usize) -> Option<Span> {
    let r = result.symbols.ref_at(offset)?;
    match r.resolved {
        ResolvedTo::Def(id) => Some(result.symbols.def(id).def_span.clone()),
        _ => None,
    }
}
