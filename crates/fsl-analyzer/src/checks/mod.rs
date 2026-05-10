//! 意味解析 Check 群
//!
//! 各 Check は `Check` trait を実装する ZST．
//! 新規検査は本ファイルの `run_all_checks` の登録列挙に追加するだけで組み込まれる．

mod reassign;
mod unresolved;

use crate::context::AnalysisContext;
use crate::Diagnostic;

/// 意味解析の単一検査
pub trait Check {
    /// 検査名 (テストやログ用)
    fn name(&self) -> &'static str;
    /// 検査本体．診断は `diags` に push する
    fn run(&self, ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>);
}

/// 全検査をリスト順に実行する
pub fn run_all_checks(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>) {
    // 新規検査はこの配列に追加するだけで連動する
    let checks: &[&dyn Check] = &[&reassign::ReassignCheck, &unresolved::UnresolvedCheck];
    for c in checks {
        c.run(ctx, diags);
    }
}
