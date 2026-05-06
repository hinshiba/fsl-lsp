use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just},
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    Block, CompositeDef, CompositeField, Expr, Field, FnBodyKind, FnDef, Ident, InstanceDecl,
    OutputFnDecl, Param, PortDecl, StageDef, StageItem, StateDef, Stmt, StmtKind, ValDecl, ValLhs,
    parsers::{
        atom::ident_def,
        block::block_def,
        expr::expr_def,
        stmt::stmt_def,
        typedef::type_def,
        valdecl::{mem_def, reg_def},
    },
};

/// モジュール／トレイト本体に並ぶフィールド列
pub(super) fn fields_def<'tok, I>()
-> impl Parser<'tok, I, Vec<Spanned<Field>>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let field = choice((
        // 実質的にコンストラクタ宣言
        input_def().map(Field::Input),
        output_def(),
        // フィールド変数
        reg_def().map(Field::Reg),
        mem_def().map(Field::Mem),
        val_or_instance_def(),
        // メソッド
        fn_def().map(Field::Fn),
        stage_def().map(Field::Stage),
        // その他ブロック
        always_def().map(Field::Always),
        initial_def().map(Field::Initial),
        // typeによる複合型宣言
        composite_def().map(Field::Composite),
    ));

    field
        .map_with(|f, e| Spanned {
            inner: f,
            span: e.span(),
        })
        .repeated()
        .collect()
}

/// 入力ポート宣言: `input name: T`
pub(super) fn input_def<'tok, I>() -> impl Parser<'tok, I, PortDecl, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Input)
        // ポート名
        .ignore_then(ident_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|(name, ty)| PortDecl { name, ty })
}

/// 出力アイテム: `output name: T` または `output def name(params): T`
///
/// 形態が分岐するため `Field` を直接返す．
pub(super) fn output_def<'tok, I>() -> impl Parser<'tok, I, Field, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 出力関数
    let output_fn = just(Token::Def)
        // 関数名
        .ignore_then(ident_def())
        // パラメータ列
        .then(params_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|((name, params), ret)| Field::OutputFn(OutputFnDecl { name, params, ret }));

    // 出力ポート
    let output_port = ident_def()
        // ポート名
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|(name, ty)| Field::Output(PortDecl { name, ty }));

    // 関数はdefが来るという点で曖昧さがすぐに解消できるので先に試みる
    just(Token::Output).ignore_then(choice((output_fn, output_port)))
}

/// 関数定義: `[private] def [recv.]name(params)[: T] (= body | seq block | par block)`
/// FSL チュートリアル 1.5.4 p60にrecv.の用法あり
pub(super) fn fn_def<'tok, I>() -> impl Parser<'tok, I, FnDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 単一式を Block へ昇格
    let expr_body = expr_def().map(|e| Block {
        stmts: vec![Stmt {
            kind: StmtKind::Expr(e),
        }],
    });

    // 関数は = {block}, = expression, seq {block}, par {block}をとる
    let body = choice((
        just(Token::Eq)
            .ignore_then(choice((block_def(), expr_body)))
            .map(|b| (FnBodyKind::Expr, b)),
        just(Token::Seq)
            .ignore_then(block_def())
            .map(|b| (FnBodyKind::Seq, b)),
        just(Token::Par)
            .ignore_then(block_def())
            .map(|b| (FnBodyKind::Par, b)),
    ));

    // 内部関数か
    just(Token::Private)
        .or_not()
        .then_ignore(just(Token::Def))
        // 関数名か`recv`
        .then(ident_def())
        // .関数名
        .then(just(Token::Dot).ignore_then(ident_def()).or_not())
        // パラメータ列
        .then(params_def())
        // : 戻り値の型 parやseqにはない
        .then(just(Token::Colon).ignore_then(type_def()).or_not())
        // 本体
        .then(body)
        .map(
            |(((((priv_kw, first), recv_opt), params), ret), (body_kind, body))| {
                // 2回目のidentがあるかどうかで分岐
                let (receiver, name) = match recv_opt {
                    Some(real) => (Some(first), real),
                    None => (None, first),
                };
                FnDef {
                    is_private: priv_kw.is_some(),
                    receiver,
                    name,
                    params,
                    ret,
                    body_kind,
                    body,
                }
            },
        )
}

/// `always { block }`
pub(super) fn always_def<'tok, I>() -> impl Parser<'tok, I, Block, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Always).ignore_then(block_def())
}

/// `initial { block }`
pub(super) fn initial_def<'tok, I>() -> impl Parser<'tok, I, Block, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Initial).ignore_then(block_def())
}

/// stage 定義: `stage name(params) { stage_items }`
pub(super) fn stage_def<'tok, I>() -> impl Parser<'tok, I, StageDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Stage)
        // ステージ名
        .ignore_then(ident_def())
        // パラメータ列
        .then(params_def())
        // ステージ本体
        .then(
            stage_item_def()
                .spanned()
                .repeated()
                .collect()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((name, params), body)| StageDef { name, params, body })
}

/// 複合型定義: `type name(field: T, ...)`
pub(super) fn composite_def<'tok, I>()
-> impl Parser<'tok, I, CompositeDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let composite_field = ident_def()
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|(name, ty)| CompositeField { name, ty });

    just(Token::Type)
        .ignore_then(ident_def())
        .then(
            composite_field
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .map(|(name, fields)| CompositeDef { name, fields })
}

/// `val pat[: T] = (new ModName | expr)`
///
/// 単独識別子に対する `new` のみ `Field::Instance` に振り分け，他は `Field::Val` とする．
pub(super) fn val_or_instance_def<'tok, I>()
-> impl Parser<'tok, I, Field, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    enum ValRhs {
        Instance(Ident),
        Expr(Expr),
    }

    just(Token::Val).ignore_then(choice((
        // タプル宣言と代入 型は書かない前提であることに注意
        val_tuple_def()
            .then_ignore(just(Token::Eq))
            .then(expr_def())
            .map(|(lhs, expr)| {
                Field::Val(ValDecl {
                    pattern: ValLhs::Tuple(lhs),
                    ty: None,
                    init: expr,
                })
            }),
        // val ident =
        ident_def()
            .then(just(Token::Colon).ignore_then(type_def()).or_not())
            .then_ignore(just(Token::Eq))
            .then(choice((
                // new
                just(Token::New)
                    .ignore_then(ident_def())
                    .map(ValRhs::Instance),
                // expr
                expr_def().map(ValRhs::Expr),
            )))
            .map(|((ident, ty), rhs)| match rhs {
                ValRhs::Expr(e) => Field::Val(ValDecl {
                    pattern: ValLhs::Single(ident),
                    ty: ty,
                    init: e,
                }),
                ValRhs::Instance(i) => Field::Instance(InstanceDecl {
                    name: ident,
                    module_name: i,
                }),
            }),
    )))
}

// ============================================================
// 局所ヘルパー
// ============================================================

/// パラメータ単体: `name[: T]`
fn param_def<'tok, I>() -> impl Parser<'tok, I, Param, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 仮引数名
    ident_def()
        // 型
        .then(just(Token::Colon).ignore_then(type_def()).or_not())
        .map(|(name, ty)| Param { name, ty })
}

/// パラメータ列: `(p1, p2, ...)`
fn params_def<'tok, I>() -> impl Parser<'tok, I, Vec<Spanned<Param>>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // パラメータ
    param_def()
        .spanned()
        // p1, p2
        .separated_by(just(Token::Comma))
        .collect()
        // (...)
        .delimited_by(just(Token::LParen), just(Token::RParen))
}

/// タプル宣言 val (a, b, c) = (1, 2, 3) 式
fn val_tuple_def<'tok, I>() -> impl Parser<'tok, I, Vec<Ident>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    ident_def()
        // ident, ident, ..., ident
        .separated_by(just(Token::Comma))
        .collect()
        // (...)
        .delimited_by(just(Token::LParen), just(Token::RParen))
}

/// `val pat[: T] = expr` stage 内用のためインスタンス宣言がない
fn val_decl_def<'tok, I>() -> impl Parser<'tok, I, ValDecl, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Val)
        .ignore_then(val_tuple_def())
        .then(just(Token::Colon).ignore_then(type_def()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr_def())
        .map(|((pattern, ty), init)| ValDecl {
            pattern: ValLhs::Tuple(pattern),
            ty,
            init,
        })
}

/// stage 本体に出現する1要素
fn stage_item_def<'tok, I>() -> impl Parser<'tok, I, StageItem, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    choice((
        state_def().map(StageItem::State),
        reg_def().map(StageItem::Reg),
        mem_def().map(StageItem::Mem),
        val_decl_def().map(StageItem::Val),
        stmt_def().map(StageItem::Stmt),
    ))
}

/// stage 内の state 定義: `state name <stmt>`
fn state_def<'tok, I>() -> impl Parser<'tok, I, StateDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::State)
        // ステート名
        .ignore_then(ident_def())
        // 本体となる文
        .then(stmt_def())
        .map(|(name, body)| StateDef { name, body })
}
