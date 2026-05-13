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
    OutputFnDecl, Param, PortDecl, StageDef, StageItem, StateDef, ValDecl, ValLhs,
    parsers::{
        RecBlock, RecExpr,
        atom::ident_def,
        block::stmt_def,
        block_and_expr,
        typedef::type_def,
        valdecl::{mem_def, reg_def, val_decl_def, val_tuple_def},
    },
};

/// モジュール／トレイト本体に並ぶフィールド列
///
/// Block と Expr の相互再帰ハンドルをここで一度だけ作って
/// 必要な子パーサに `.clone()` で配る  各フィールドはこの一組の
/// 再帰パーサを共有するため，深いネストでも一貫して同じパーサが走る
pub(super) fn fields_def<'tok, I>()
-> impl Parser<'tok, I, Vec<Spanned<Field>>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let (block, expr) = block_and_expr::<'tok, I>();

    let field = choice((
        // 実質的にコンストラクタ宣言
        input_def().map(Field::Input),
        output_def(),
        // フィールド変数
        reg_def(expr.clone()).map(Field::Reg),
        mem_def(expr.clone()).map(Field::Mem),
        val_or_instance_def(expr.clone()),
        // メソッド
        fn_def(block.clone(), expr.clone()).map(Field::Fn),
        stage_def(block.clone(), expr.clone()).map(Field::Stage),
        // その他ブロック
        always_def(block.clone()).map(Field::Always),
        initial_def(block.clone()).map(Field::Initial),
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
///
/// 再帰ハンドラを外から注入
pub(super) fn fn_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, FnDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 単一式は擬似的な単文ブロックに昇格する
    // Spanned<Statement> を作るため map_with で span を取る
    let expr_body = expr.clone().map_with(|e, info| Block {
        stmts: vec![Spanned {
            inner: crate::Statement::Expr(e),
            span: info.span(),
        }],
    });

    // 関数は = {block}, = expression, seq {block}, par {block}をとる
    let body = choice((
        just(Token::Eq)
            .ignore_then(choice((block.clone(), expr_body)))
            .map(|b| (FnBodyKind::Expr, b)),
        just(Token::Seq)
            .ignore_then(block.clone())
            .map(|b| (FnBodyKind::Seq, b)),
        just(Token::Par)
            .ignore_then(block.clone())
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
pub(super) fn always_def<'tok, I>(
    block: RecBlock<'tok, I>,
) -> impl Parser<'tok, I, Block, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Always).ignore_then(block)
}

/// `initial { block }`
pub(super) fn initial_def<'tok, I>(
    block: RecBlock<'tok, I>,
) -> impl Parser<'tok, I, Block, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Initial).ignore_then(block)
}

/// stage 定義: `stage name(params) { stage_items }`
pub(super) fn stage_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StageDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // ステージ本体  block 同様に文末・先頭セミコロンを任意で許容する
    let body = stage_item_def(block.clone(), expr.clone())
        .spanned()
        .separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    just(Token::Stage)
        // ステージ名
        .ignore_then(ident_def())
        // パラメータ列
        .then(params_def())
        // ステージ本体
        .then(body)
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
pub(super) fn val_or_instance_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Field, extra::Err<Rich<'tok, Token>>>
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
            .then(expr.clone())
            .map(|(lhs, e)| {
                Field::Val(ValDecl {
                    pattern: ValLhs::Tuple(lhs),
                    ty: None,
                    init: e,
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
                expr.clone().map(ValRhs::Expr),
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

/// stage 本体に出現する1要素
///
/// `state` 定義  `reg`/`mem`/`val` 宣言  およびステージ内の制御文が混在する
fn stage_item_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StageItem, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    choice((
        state_def(block.clone(), expr.clone()).map(StageItem::State),
        reg_def(expr.clone()).map(StageItem::Reg),
        mem_def(expr.clone()).map(StageItem::Mem),
        val_decl_def(expr.clone()).map(StageItem::Val),
        // それ以外は単独の文として stage に組み込む
        stmt_def(block.clone(), expr.clone()).map(StageItem::Statement),
    ))
}

/// stage 内の state 定義: `state name <stmt>`
fn state_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StateDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::State)
        // ステート名
        .ignore_then(ident_def())
        // 本体  通常は par/seq/any/alt ブロック文だが任意の文を許容する
        .then(stmt_def(block, expr))
        .map(|(name, body)| StateDef { name, body })
}
