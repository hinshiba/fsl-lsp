//! 未宣言シンボル検査
//!
//! `Reference` のうち `ResolvedTo::Unresolved` になっているものをエラーに変換する．

use super::Check;
use crate::context::AnalysisContext;
use crate::symbols::ResolvedTo;
use crate::{Diagnostic, Severity};

pub struct UnresolvedCheck;

impl Check for UnresolvedCheck {
    fn name(&self) -> &'static str {
        "unresolved"
    }

    fn run(&self, ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>) {
        for r in ctx.references {
            if matches!(r.resolved, ResolvedTo::Unresolved) {
                diags.push(Diagnostic {
                    severity: Severity::Error,
                    message: format!("`{}` は未宣言のシンボルです", r.name),
                    span: r.span.clone(),
                });
            }
        }
    }
}
