//! 解析コンテキスト
//!
//! 各 Check に渡す read-only コンテキスト．
//! AST・シンボルテーブル・参照集合・ビルトイン定義を束ねる．

use fsl_parser::CompilationUnit;

use crate::builtin::Builtins;
use crate::symbols::{Reference, SymbolTable};

/// Check 実行に必要な解析結果のスナップショット
pub struct AnalysisContext<'a> {
    pub unit: &'a CompilationUnit,
    pub symbols: &'a SymbolTable,
    pub references: &'a [Reference],
    pub builtins: &'static Builtins,
}
