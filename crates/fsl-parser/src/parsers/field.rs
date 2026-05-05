use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just, todo},
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    Field, Item, ModuleDef, RegDecl, TraitDef,
    parsers::{
        atom::{self, ident_def},
        expr::expr_def,
        typedef::type_def,
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
