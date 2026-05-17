//! Hover
//!
//! `offset` 上のシンボルから hover 表示用の Markdown を生成する．
//! 型情報があればコードブロックで，無ければ kind + name の見出しを返す．

use crate::span::Span;
use crate::symbol::Symbol;
use crate::symbols::ResolvedTo;
use crate::AnalysisResult;

/// hover の戻り値
pub struct HoverPayload {
    /// Markdown 文字列
    pub markdown: String,
    /// 強調範囲 (識別子の span)
    pub range: Span,
}

/// `offset` 上のシンボルから hover 表示を生成
/// 参照ヒットでも定義ヒットでも同形式の Markdown を返す
pub fn hover_at(result: &AnalysisResult, offset: usize) -> Option<HoverPayload> {
    // 参照位置のホバー
    if let Some(r) = result.symbols.ref_at(offset) {
        let range = r.span.clone();
        return match r.resolved {
            ResolvedTo::Def(id) => {
                let s = result.symbols.def(id);
                Some(HoverPayload {
                    markdown: format_symbol(s),
                    range,
                })
            }
            ResolvedTo::Builtin(name) => Some(HoverPayload {
                markdown: format_builtin(name),
                range,
            }),
            // 継承メンバ・未解決は定義位置を持たないため hover しない
            ResolvedTo::External | ResolvedTo::Unresolved => None,
        };
    }
    // 宣言識別子上のホバー
    if let Some(s) = result.symbols.def_at(offset) {
        return Some(HoverPayload {
            markdown: format_symbol(s),
            range: s.def_span.clone(),
        });
    }
    None
}

// ============================================================
// 整形
// ============================================================

fn format_symbol(s: &Symbol) -> String {
    if let Some(t) = &s.ty {
        format!("```fsl\n{} {}: {}\n```", s.kind.label(), s.name, t)
    } else {
        format!("**{}** `{}`", s.kind.label(), s.name)
    }
}

fn format_builtin(name: &str) -> String {
    let sig = match name {
        "_display" => "_display(fmt: String, args*): Unit",
        "_finish" => "_finish(fmt: String, args*): Unit",
        "_time" => "_time: Int",
        _ => name,
    };
    format!("```fsl\n{}\n```", sig)
}
