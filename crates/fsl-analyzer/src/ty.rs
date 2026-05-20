//! 型情報
//!
//! AST の `FslType` を analyzer 寄りの軽量表現に変換する．
//! ビット幅式の評価は将来作業のため，現状は元ソース風の文字列で保持する．

use fsl_parser::{Expr_, FslType, FslType_};
use std::fmt;

/// hover 表示や型検査で扱う型表現
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    Unit,
    Boolean,
    Int,
    String,
    /// `Bit(n)` の n は意味解析未実装のため文字列保持
    Bit(String),
    Array(Box<TypeInfo>),
    List(Box<TypeInfo>),
    Tuple(Vec<TypeInfo>),
    /// ユーザ定義型・モジュール名・trait 名
    Named(String),
    /// 解析失敗時のフォールバック
    Unknown,
}

impl TypeInfo {
    /// AST の型ノードを `TypeInfo` に変換する純粋関数
    pub fn from_ast(t: &FslType) -> Self {
        match &t.inner {
            FslType_::Unit => TypeInfo::Unit,
            FslType_::Boolean => TypeInfo::Boolean,
            FslType_::Int => TypeInfo::Int,
            FslType_::String => TypeInfo::String,
            FslType_::Bit(e) => TypeInfo::Bit(format_expr(&e.inner)),
            FslType_::Array(inner) => TypeInfo::Array(Box::new(TypeInfo::from_ast(inner))),
            FslType_::List(inner) => TypeInfo::List(Box::new(TypeInfo::from_ast(inner))),
            FslType_::Tuple(items) => {
                TypeInfo::Tuple(items.iter().map(TypeInfo::from_ast).collect())
            }
            FslType_::Named(id) => TypeInfo::Named(id.inner.clone()),
        }
    }
}

/// hover 表示用の式の簡易整形．完全な式復元ではない
fn format_expr(e: &Expr_) -> String {
    match e {
        Expr_::IntLit(n) | Expr_::BitLit(n) => n.to_string(),
        Expr_::Variable(id) => id.inner.clone(),
        _ => "?".into(),
    }
}

impl fmt::Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeInfo::Unit => write!(f, "Unit"),
            TypeInfo::Boolean => write!(f, "Boolean"),
            TypeInfo::Int => write!(f, "Int"),
            TypeInfo::String => write!(f, "String"),
            TypeInfo::Bit(n) => write!(f, "Bit({})", n),
            TypeInfo::Array(inner) => write!(f, "Array[{}]", inner),
            TypeInfo::List(inner) => write!(f, "List[{}]", inner),
            TypeInfo::Tuple(items) => {
                write!(f, "(")?;
                for (i, t) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            TypeInfo::Named(name) => write!(f, "{}", name),
            TypeInfo::Unknown => write!(f, "?"),
        }
    }
}
