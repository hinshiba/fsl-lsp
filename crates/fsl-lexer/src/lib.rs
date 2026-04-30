//! FSL のトークナイザ
//!
//! `logos` を用いてソースコードをトークン列に変換する．
//! 改行とコメントは破棄せず保持する．フォーマッタとASI戦略のため．

use logos::Logos;

pub type Span = std::ops::Range<usize>;

/// トークン種別
///
/// 数値・文字列・識別子はソース文字列を `String` で保持する．
/// AST 段階で意味解析を行うため，リテラルの解釈はここでは行わない．
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // ---- 宣言キーワード ----
    #[token("module")]
    Module,
    #[token("trait")]
    Trait,
    #[token("stage")]
    Stage,
    #[token("state")]
    State,
    #[token("def")]
    Def,
    #[token("type")]
    Type,
    #[token("val")]
    Val,
    #[token("reg")]
    Reg,
    #[token("mem")]
    Mem,
    #[token("input")]
    Input,
    #[token("output")]
    Output,
    #[token("new")]
    New,

    // ---- 修飾子キーワード ----
    #[token("private")]
    Private,
    #[token("extends")]
    Extends,
    #[token("with")]
    With,

    // ---- 制御キーワード ----
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("match")]
    Match,
    #[token("case")]
    Case,
    #[token("par")]
    Par,
    #[token("seq")]
    Seq,
    #[token("any")]
    Any,
    #[token("alt")]
    Alt,
    #[token("always")]
    Always,
    #[token("initial")]
    Initial,
    #[token("generate")]
    Generate,
    #[token("relay")]
    Relay,
    #[token("finish")]
    Finish,
    #[token("goto")]
    Goto,

    // ---- 真偽値リテラル ----
    #[token("true")]
    True,
    #[token("false")]
    False,

    // ---- 区切り記号 ----
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,
    #[token(".")]
    Dot,

    // ---- 演算子 ----
    #[token(":=")]
    ColonEq,
    #[token("=>")]
    FatArrow,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token(">>>")]
    ShrLogical,
    #[token(">>")]
    Shr,
    #[token("<<")]
    Shl,
    #[token("&&")]
    AmpAmp,
    #[token("||")]
    PipePipe,
    #[token("++")]
    PlusPlus,
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("!")]
    Bang,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("#")]
    Hash,

    // ---- リテラル ----
    #[regex(r"0b[01_]+", |lex| lex.slice().to_string())]
    #[regex(r"0x[0-9a-fA-F_]+", |lex| lex.slice().to_string())]
    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().to_string())]
    IntLit(String),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice().to_string())]
    StringLit(String),

    /// 識別子．予約語より低優先となるよう priority を明示する．
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string(), priority = 1)]
    Ident(String),

    // ---- トリビア ----
    #[regex(r"//[^\n]*", |lex| lex.slice().to_string(), allow_greedy = true)]
    LineComment(String),

    /// ブロックコメント．ネストは想定しない．
    #[regex(r"/\*", block_comment)]
    BlockComment(String),

    /// 改行はフォーマッタとASI戦略のため独立トークンとして保持する．
    #[token("\n")]
    Newline,

    #[regex(r"[ \t\r]+", logos::skip)]
    Whitespace,
}

fn block_comment(lex: &mut logos::Lexer<Token>) -> String {
    let remainder: &str = lex.remainder();
    let consumed = match remainder.find("*/") {
        Some(end) => end + 2,
        None => remainder.len(),
    };
    lex.bump(consumed);
    let span = lex.span();
    lex.source()[span.start..span.end].to_string()
}

/// ソースをトークン列に分解する．字句エラー位置を別途返す．
pub fn lex(src: &str) -> (Vec<(Token, Span)>, Vec<Span>) {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut lexer = Token::lexer(src);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        match result {
            Ok(tok) => tokens.push((tok, span)),
            Err(_) => errors.push(span),
        }
    }
    (tokens, errors)
}

/// パーサに渡す前のフィルタ．コメントと改行を除去する．
pub fn strip_trivia(tokens: Vec<(Token, Span)>) -> Vec<(Token, Span)> {
    tokens
        .into_iter()
        .filter(|(t, _)| {
            !matches!(
                t,
                Token::LineComment(_) | Token::BlockComment(_) | Token::Newline
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Token> {
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "lex errors: {:?}", errs);
        toks.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn keywords() {
        let toks = kinds("module trait stage state def val reg mem input output new private extends with");
        assert_eq!(toks[0], Token::Module);
        assert_eq!(toks.last().unwrap(), &Token::With);
    }

    #[test]
    fn integer_literals() {
        let toks = kinds("0 100 0b1010 0xFF 1_000");
        let lits: Vec<_> = toks
            .into_iter()
            .filter_map(|t| match t {
                Token::IntLit(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(lits, vec!["0", "100", "0b1010", "0xFF", "1_000"]);
    }

    #[test]
    fn operators_multichar() {
        let toks = kinds(":= => == != <= >= >>> >> << && || ++");
        assert_eq!(
            toks,
            vec![
                Token::ColonEq,
                Token::FatArrow,
                Token::EqEq,
                Token::NotEq,
                Token::Le,
                Token::Ge,
                Token::ShrLogical,
                Token::Shr,
                Token::Shl,
                Token::AmpAmp,
                Token::PipePipe,
                Token::PlusPlus,
            ]
        );
    }

    #[test]
    fn comments_preserved() {
        let toks = kinds("// hello\n/* block */ x");
        assert!(matches!(toks[0], Token::LineComment(_)));
        assert!(matches!(toks[1], Token::Newline));
        assert!(matches!(toks[2], Token::BlockComment(_)));
        assert!(matches!(toks[3], Token::Ident(_)));
    }

    #[test]
    fn unterminated_block_comment() {
        let (toks, errs) = lex("/* never closed");
        assert!(errs.is_empty());
        assert!(matches!(toks.last().unwrap().0, Token::BlockComment(_)));
    }
}
