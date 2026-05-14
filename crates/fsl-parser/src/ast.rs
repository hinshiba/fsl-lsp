//! FSL の抽象構文木定義
//!
//! チュートリアル例から逆算した文法に対応する．
//! ノードはソース上の位置情報 `Span` を持つ．

use chumsky::span::Spanned;

pub type Ident = Spanned<String>;

// ============================================================
// トップレベル
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompilationUnit {
    pub items: Vec<Spanned<Item>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Trait(TraitDef),
    Module(ModuleDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: Ident,
    pub items: Vec<Spanned<Field>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub with_traits: Option<Vec<Ident>>,
    pub items: Vec<Spanned<Field>>,
}

// ============================================================
// モジュールアイテム
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    // ---- 入出力端子 ----
    Input(InputDecl),
    Output(OutputDecl),
    OutputFn(OutputFnDecl),

    // ---- val ----
    Val(ValDecl),
    NewInstance(NewInstance),

    // ---- 記憶素子 ----
    Reg(RegDecl),
    Mem(MemDecl),

    // ---- 型宣言 ----
    Composite(CompositeDef),

    // ---- 内容 ----
    Always(Expr),
    Initial(Expr),
    Fn(FnDef),
    Stage(StageDef),

    /// 解析失敗時のプレースホルダ
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Ident,
    /// 引数型
    pub ty: Option<FslType>,
}

// ============================================================
// 定義
// ============================================================

/// 関数の定義
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDef {
    pub is_private: bool,
    /// `cpu.mem_read` のように `<inst>.<name>` 形式の上書き定義に対応
    pub receiver: Option<Ident>,
    pub name: Ident,
    pub params: Vec<Spanned<Param>>,
    pub ret: Option<FslType>,
    pub body: Expr,
}

/// ステージの定義
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageDef {
    pub name: Ident,
    pub params: Vec<Spanned<Param>>,
    pub body: Vec<Spanned<StageItem>>,
}

/// stage 本体にはreg宣言, relayとfinishによるタスク処理, ステートマシンと実装が含まれる
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageItem {
    Reg(RegDecl),
    Relay(Ident, Vec<Expr>),
    Finish,
    State(StateDef),
    Goto(Ident),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDef {
    pub name: Ident,
    pub body: Expr,
}

/// 複合型定義
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeDef {
    pub name: Ident,
    pub fields: Vec<CompositeField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeField {
    pub name: Ident,
    pub ty: FslType,
}

// ============================================================
// 宣言
// ============================================================

/// valによる宣言
/// 唯一 newを右にとれる
/// 不変であるので初期化子が必須
/// module: o, func: o, stage: o
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValDecl {
    pub pattern: ValLhs,
    pub ty: Option<FslType>,
    pub init: Box<Expr>,
}

/// `val`左辺
/// 単一の変数宣言と，タプルによる宣言がある
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValLhs {
    Single(Ident),
    Tuple(Vec<Ident>),
}

/// regによる宣言
/// module: o, func: x, stage: o
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegDecl {
    pub name: Ident,
    pub ty: FslType,
    pub init: Option<Expr>,
}

/// memによる宣言
/// module: o, func: x, stage: x
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemDecl {
    pub name: Ident,
    pub elem_ty: FslType,
    pub size: Expr,
    pub init: Vec<Expr>,
}

/// inputによる宣言
/// module: o, func: x, stage: x
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDecl {
    pub name: Ident,
    pub ty: FslType,
}

/// outputによる宣言
/// module: o, func: x, stage: x
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDecl {
    pub name: Ident,
    pub ty: FslType,
}

/// output def による宣言
/// module: o, func: x, stage: x
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFnDecl {
    pub name: Ident,
    pub params: Vec<Spanned<Param>>,
    pub ret: FslType,
}

/// new によるインスタンス化
/// module: o, func: x, stage: x (TODO)
/// val ident = new module
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstance {
    pub name: Ident,
    pub module_name: Ident,
}

// ============================================================
// 式
// ============================================================

pub type Expr = Spanned<Expr_>;

// Spanned<>にするのを一括で行うため_をつけてある
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr_ {
    Unit,
    // ---- リテラル ----
    IntLit(u64),
    BitLit(u64),
    StringLit(String),
    Bool(bool),

    /// 変数
    Variable(Ident),

    /// `(e1, e2, ...)` または単独の括弧式
    Tuple(Vec<Expr>),

    // ---- 演算 ----
    /// 単項 `~` `!` `-`
    Unary(UnaryOp, Box<Expr>),
    /// 二項演算
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `f(args)` 関数呼び出し or ビット切り出し
    /// 意味解析で区別する
    Call(Box<Expr>, Vec<Expr>),

    // ---- 構造 ----
    /// `expr.field`
    Field(Box<Expr>, Ident),
    /// `if (cond) then else else_`
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    /// `e match { case p => e ... }`
    Match(Box<Expr>, Vec<MatchArm>),
    /// 単独ブロック `{ ... }` を式として
    Block(Block),
    /// `new ModName`
    New(Ident),
    /// 解析失敗時のプレースホルダ
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum UnaryOp {
    /// ビットNOT
    BitNot,
    /// 論理NOT
    LogNot,
    /// 単項マイナス
    Neg,
    /// リダクション論理和 `|x`（パターン上は二項 `|` と区別が必要）
    RedOr,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum BinaryOp {
    LogOr,
    LogAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
    ShrLogical,
    Concat,
    Add,
    Sub,
    Mul,
    /// 符号拡張 `n # x`
    SignExt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// `case _ =>`
    Wildcard,
    /// `case ADD =>` 識別子（定数または束縛）
    Ident(Ident),
    /// `case 0x00 =>` リテラル
    IntLit(String),
}
