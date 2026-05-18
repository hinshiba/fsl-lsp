//! モジュール／トレイト本体に並ぶフィールドのパーサ

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{any, choice, just, none_of, one_of},
    recovery::via_parser,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    CompositeDef, CompositeField, Expr, Field, FnDef, Ident, InputDecl, NewInstance, OutputDecl,
    OutputFnDecl, Param, StageDef, StageItem, StateDef, ValDecl, ValLhs,
    parsers::{
        RecExpr,
        atom::ident_def,
        block::block_def,
        rbrace,
        typedef::type_def,
        valdecl::{mem_def, reg_def, val_tuple_def},
    },
};

/// モジュール／トレイト本体に並ぶフィールド列
///
/// `expr` の再帰ハンドルを受け取り，必要な子パーサに `.clone()` で配る．
/// 各フィールドは失敗時に `Field::Error` へ復旧し，後続フィールドの解析を続ける．
pub(super) fn fields_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Vec<Spanned<Field>>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let field = choice((
        // 実質的にコンストラクタ宣言
        input_def().map(Field::Input),
        output_def(),
        // フィールド変数
        reg_def(expr.clone()).map(Field::Reg),
        mem_def(expr.clone()).map(Field::Mem),
        val_or_instance_def(expr.clone()),
        // メソッド
        fn_def(expr.clone()).map(Field::Fn),
        stage_def(expr.clone()).map(Field::Stage),
        // その他ブロック
        always_def(expr.clone()).map(Field::Always),
        initial_def(expr.clone()).map(Field::Initial),
        // typeによる複合型宣言
        composite_def().map(Field::Composite),
    ))
    .map_with(|f, e| Spanned {
        inner: f,
        span: e.span(),
    });

    field
        .recover_with(via_parser(field_recovery()))
        .repeated()
        .collect()
}

/// 入力ポート宣言: `input name: T`
pub(super) fn input_def<'tok, I>() -> impl Parser<'tok, I, InputDecl, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Input)
        // ポート名
        .ignore_then(ident_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|(name, ty)| InputDecl { name, ty })
}

/// 出力アイテム: `output name: T` または `output def name(params): T`
///
/// 形態が分岐するため `Field` を直接返す．
pub(super) fn output_def<'tok, I>() -> impl Parser<'tok, I, Field, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 出力関数  戻り値型は省略可能
    let output_fn = just(Token::Def)
        // 関数名
        .ignore_then(ident_def())
        // パラメータ列
        .then(params_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()).or_not())
        .map(|((name, params), ret)| Field::OutputFn(OutputFnDecl { name, params, ret }));

    // 出力ポート
    let output_port = ident_def()
        // ポート名
        .then(just(Token::Colon).ignore_then(type_def()))
        .map(|(name, ty)| Field::Output(OutputDecl { name, ty }));

    // 関数はdefが来るという点で曖昧さがすぐに解消できるので先に試みる
    just(Token::Output).ignore_then(choice((output_fn, output_port)))
}

/// 関数定義: `[private] def [recv.]name(params)[: T] (= expr | seq block | par block)`
/// FSL チュートリアル 1.5.4 p60にrecv.の用法あり
pub(super) fn fn_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, FnDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // `= expr` の `=` は任意  省略時は `seq`/`par` ブロック式が直接来る
    let body = just(Token::Eq).or_not().ignore_then(expr);

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
        .map(|(((((priv_kw, first), recv_opt), params), ret), body)| {
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
                body,
            }
        })
}

/// `always { ... }`  本体は brace ブロック式
pub(super) fn always_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Always).ignore_then(block_def(expr))
}

/// `initial { ... }`  本体は brace ブロック式
pub(super) fn initial_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Initial).ignore_then(block_def(expr))
}

/// stage 定義: `stage name(params) { stage_items }`
pub(super) fn stage_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StageDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // ステージ本体  brace ブロック同様に文末・先頭セミコロンを任意で許容する
    let body = stage_item_def(expr.clone())
        .spanned()
        .separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect()
        // 閉じ `}` の欠落から復旧する
        .delimited_by(just(Token::LBrace), rbrace());

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
/// 単独識別子に対する `new` のみ `Field::NewInstance` に振り分け，他は `Field::Val` とする．
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
                    init: Box::new(e),
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
                    ty,
                    init: Box::new(e),
                }),
                ValRhs::Instance(i) => Field::NewInstance(NewInstance {
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
/// `state` 定義  `reg` 宣言  およびその他の式  `val`/`par`/`relay` 等
fn stage_item_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StageItem, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    choice((
        state_def(expr.clone()).map(StageItem::State),
        reg_def(expr.clone()).map(StageItem::Reg),
        // val 宣言・代入・relay/finish/goto・par/seq/any/alt はすべて式
        expr.map(StageItem::Expr),
    ))
}

/// stage 内の state 定義: `state name <expr>`
fn state_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, StateDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::State)
        // ステート名
        .ignore_then(ident_def())
        // 本体  通常は par/seq/any/alt ブロック式
        .then(expr)
        .map(|(name, body)| StateDef { name, body })
}

/// フィールドの解析失敗からの復旧パーサ
///
/// 先頭の不正トークンを 1 つ消費し，次のフィールド開始キーワードまたは
/// `}` の手前まで読み飛ばして `Field::Error` を生成する．
fn field_recovery<'tok, I>()
-> impl Parser<'tok, I, Spanned<Field>, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 次のフィールド開始となりうるトークン  ここまでで読み飛ばしを止める
    let sync = one_of([
        Token::Input,
        Token::Output,
        Token::Reg,
        Token::Mem,
        Token::Val,
        Token::Def,
        Token::Private,
        Token::Stage,
        Token::Always,
        Token::Initial,
        Token::Type,
        Token::RBrace,
    ]);

    // `}` は消費しない  本体の閉じ括弧として上位に残す
    none_of([Token::RBrace])
        .ignore_then(any().and_is(sync.not()).repeated().collect::<Vec<_>>())
        .map_with(|_, e| Spanned {
            inner: Field::Error,
            span: e.span(),
        })
}
