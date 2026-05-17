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

/// stage 本体に並ぶ要素
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageItem {
    Reg(RegDecl),
    State(StateDef),
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

    // ---- stage ----
    // if内等にも配置されるため exprだが，stage内のみ通用する
    /// `generate <stage>(args)`  Unitを返す
    Generate(Ident, Vec<Expr>),
    /// `relay <stage>(args)`  後段ステージへの中継  Unitを返す
    Relay(Ident, Vec<Expr>),
    /// `finish`  タスクの終了  Unitを返す
    Finish,
    /// `goto <state>`  ステート遷移  Unitを返す
    Goto(Ident),

    // ---- 宣言と代入 ----
    // 普通はUnitを返す
    /// val
    ValDecl(ValDecl),

    /// input, output, valの端子への代入
    /// 左辺にビット切り出しが来る可能性あり
    PortAssign(Box<Expr>, Box<Expr>),

    /// reg, memの記憶素子への代入
    /// 現時点の仕様では左側にスライスは来ない
    MemAssign(Box<Expr>, Box<Expr>),

    // ---- ブロック関連 ----
    // 普通はUnitを返す
    /// ブロック`{ ... }`
    Block(Vec<Expr>),

    /// `any { expr:Bool : expr[;] expr:Bool : expr[;] ... else: expr[;] }`
    /// Unitを返す
    Any(Vec<Spanned<Case>>, Option<Box<Expr>>),

    /// `alt { expr:Bool : expr[;] expr:Bool : expr[;] ...  else: expr[;] }`
    /// 実行結果を返す
    Alt(Vec<Spanned<Case>>, Option<Box<Expr>>),

    /// `seq { expr expr ... }`
    /// Unitを返す
    Seq(Vec<Expr>),
    /// `par { expr expr ... }`
    /// Unitを返す
    Par(Vec<Expr>),
    /// `expr match { case p => e ... }`
    Match(Box<Expr>, Vec<Spanned<MatchArm>>),

    /// `new ModName`
    New(Ident),
    /// 解析失敗時のプレースホルダ
    Error,
}

// 演算子の表
// fsl-tutorial p18

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum UnaryOp {
    // ---- ビット演算 ----
    BitNot,
    // ---- リダクション演算 ----
    ReducAnd,
    ReducOr,
    ReducXor,

    /// 論理NOT
    LogNot,
    /// 単項マイナス
    Neg,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum BinaryOp {
    // ---- ビット演算 ----
    BitAnd,
    BitOr,
    BitXor,

    // ---- 算術演算 ----
    Add,
    Sub,
    Mul,

    // ---- 比較演算 ----
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // ---- 論理演算 ----
    LogOr,
    LogAnd,

    // ---- ビットシフト演算 ----
    Sll,
    Srl,
    Sra,

    /// ビット連結演算
    Concat,

    /// 符号拡張演算
    SignExt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub cond: Expr,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
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

// ============================================================
// 型
// ============================================================

pub type FslType = Spanned<FslType_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FslType_ {
    Unit,
    Boolean,
    Bit(Box<Expr>),
    Int,
    String,
    Array(Box<FslType>),
    List(Box<FslType>),
    Tuple(Vec<FslType>),
    /// 複合型, モジュール, traitのいずれか
    Named(Ident),
}
