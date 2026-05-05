//! FSL の抽象構文木定義
//!
//! チュートリアル例から逆算した文法に対応する．
//! ノードはソース上の位置情報 `Span` を持つ．

use chumsky::span::Spanned;
use fsl_lexer::Span;

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
    Reg(RegDecl),
    Mem(MemDecl),
    Input(PortDecl),
    Output(PortDecl),
    OutputFn(OutputFnDecl),
    Instance(InstanceDecl),
    Fn(FnDef),
    Always(Block),
    Initial(Block),
    Stage(StageDef),
    Composite(CompositeDef),
    Val(ValDecl),
    /// 解析失敗時のプレースホルダ
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegDecl {
    pub name: Ident,
    pub ty: FslType,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemDecl {
    pub name: Ident,
    pub elem_ty: FslType,
    pub size: Expr,
    pub init: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDecl {
    pub name: Ident,
    pub ty: FslType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFnDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: FslType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDecl {
    pub name: Ident,
    pub module_name: Ident,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDef {
    pub is_private: bool,
    /// `cpu.mem_read` のように `<inst>.<name>` 形式の上書き定義に対応
    pub receiver: Option<Ident>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<FslType>,
    pub body_kind: FnBodyKind,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnBodyKind {
    /// `def f(...): T = { ... }` 形式
    Expr,
    /// `def f(...) seq { ... }` 形式
    Seq,
    /// `def f(...) par { ... }` 形式
    Par,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageDef {
    pub name: Ident,
    pub params: Vec<Param>,
    pub body: Vec<StageItem>,
}

/// stage 本体には state 定義，stage ローカル変数（reg/val），文が混在する．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageItem {
    State(StateDef),
    Reg(RegDecl),
    Mem(MemDecl),
    Val(ValDecl),
    Stmt(Stmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDef {
    pub name: Ident,
    pub body: Stmt,
    pub span: Span,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValDecl {
    pub pattern: ValPattern,
    pub ty: Option<FslType>,
    pub init: Expr,
}

/// `val x = ...` または `val (a, b, c) = ...`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValPattern {
    Single(Ident),
    Tuple(Vec<Ident>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Ident,
    /// 引数型は省略可（`def add(a, b, ci): Unit` のように上位スコープから流用される）
    pub ty: Option<FslType>,
}

// ============================================================
// 型
// ============================================================

pub type FslType = Spanned<FslType_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FslType_ {
    Unit,
    Boolean,
    /// `Bit(n)` の n は意味解析で評価する．構文段階では式のまま保持．
    Bit(Box<Expr>),
    Int,
    String,
    Array(Box<FslType>),
    List(Box<FslType>),
    Tuple(Vec<FslType>),
    /// ユーザ定義型・モジュール名・trait名
    Named(Ident),
}

// ============================================================
// 文
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    Val(ValDecl),
    /// `lhs := rhs` レジスタ・メモリ更新
    RegAssign(Expr, Expr),
    /// `lhs = rhs` 出力ポート割当
    Assign(Expr, Expr),
    /// `par { ... }` `seq { ... }` `any { ... }` `alt { ... }`
    BlockKind(BlockKind, Block),
    Generate(Ident, Vec<Expr>),
    Relay(Ident, Vec<Expr>),
    Finish,
    Goto(Ident),
    /// match 文・case 節含むので Stmt と Expr の両方で構成可能
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum BlockKind {
    Par,
    Seq,
    Any,
    Alt,
}

// ============================================================
// 式
// ============================================================

pub type Expr = Spanned<Expr_>;

// Spanned<>にするのを一括で行うため_をつけてある
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr_ {
    /// 整数リテラルの未解釈ソース文字列
    Int(String),
    Str(String),
    Bool(bool),
    /// 識別子参照
    Path(Ident),
    /// `(e1, e2, ...)` または単独の括弧式
    Tuple(Vec<Expr>),
    Unit,
    /// 単項 `~` `!` `-`
    Unary(UnaryOp, Box<Expr>),
    /// 二項演算
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `f(args)` 関数呼び出し兼ビット切り出し（意味解析で区別）
    Call(Box<Expr>, Vec<Expr>),
    /// `e.field`
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
