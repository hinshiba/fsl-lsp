//! FSL の構文解析器
//!
//! `fsl-lexer` のトークン列を AST に変換する．

pub mod ast;
pub mod parser;

pub use ast::*;
pub use parser::{ParseError, ParseResult, parse};

pub use fsl_lexer::Span;

use fsl_lexer::Token;

/// ソース文字列を直接受け取るエントリポイント．
pub fn parse_source(src: &str) -> (ParseResult, Vec<Span>) {
    let (raw_tokens, lex_errors) = fsl_lexer::lex(src);
    let tokens = fsl_lexer::strip_trivia(raw_tokens);
    let result = parse(tokens, src.len());
    (result, lex_errors)
}

/// パーサが受け取る形式に整形したトークンを得る．
pub fn lex_for_parser(src: &str) -> (Vec<(Token, Span)>, Vec<Span>) {
    let (raw, errs) = fsl_lexer::lex(src);
    (fsl_lexer::strip_trivia(raw), errs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> CompilationUnit {
        let (result, lex_errs) = parse_source(src);
        assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
        assert!(
            result.errors.is_empty(),
            "parse errors: {:?}",
            result.errors
        );
        result.unit
    }

    #[test]
    fn empty_source() {
        let unit = parse_ok("");
        assert!(unit.items.is_empty());
    }

    #[test]
    fn helloworld() {
        let src = r#"module HelloWorld extends Simulator {
  reg count: Bit(8)
  always {
    count := count + 1
    if (count == 100) _display("hello")
  }
}
"#;
        let unit = parse_ok(src);
        assert_eq!(unit.items.len(), 1);
        let m = match &unit.items[0] {
            Item::Module(m) => m,
            _ => panic!("expected module"),
        };
        assert_eq!(m.name.node, "HelloWorld");
        assert!(m.extends.is_some());
    }

    #[test]
    fn fun_module() {
        let src = r#"module Fun extends Simulator {
  always {
    if (_time >= 100) _finish("result = %d", fun(_time.toBit(8)))
  }
  private def fun(value: Bit(8)): Bit(8) = {
    value + 5
  }
}
"#;
        let unit = parse_ok(src);
        assert_eq!(unit.items.len(), 1);
    }

    #[test]
    fn instance_decl() {
        let src = "module Top extends Simulator { val s = new Sub }";
        let unit = parse_ok(src);
        let m = match &unit.items[0] {
            Item::Module(m) => m,
            _ => panic!(),
        };
        assert!(matches!(m.items[0], ModuleItem::Instance(_)));
    }

    #[test]
    fn trait_def() {
        let src = r#"trait Inst {
  val ADD = 0x00
  val LD = 0x01
}
"#;
        let unit = parse_ok(src);
        assert_eq!(unit.items.len(), 1);
        match &unit.items[0] {
            Item::Trait(t) => assert_eq!(t.name.node, "Inst"),
            _ => panic!(),
        }
    }

    #[test]
    fn match_expression() {
        let src = r#"module M {
  def f(x): Bit(8) = x match {
    case 0 => 1
    case 1 => 2
    case _ => 0
  }
}
"#;
        let unit = parse_ok(src);
        assert_eq!(unit.items.len(), 1);
    }

    #[test]
    fn precedence() {
        // 優先順位: a + b * c == a + (b * c)
        let src = r#"module M {
  def f(): Bit(8) = a + b * c
}
"#;
        let unit = parse_ok(src);
        let m = match &unit.items[0] {
            Item::Module(m) => m,
            _ => panic!(),
        };
        let f = match &m.items[0] {
            ModuleItem::Fn(f) => f,
            _ => panic!(),
        };
        let body_stmt = &f.body.stmts[0];
        let expr = match &body_stmt.kind {
            StmtKind::Expr(e) => e,
            _ => panic!(),
        };
        match &expr.kind {
            ExprKind::Binary(BinaryOp::Add, _, rhs) => match &rhs.kind {
                ExprKind::Binary(BinaryOp::Mul, _, _) => {}
                k => panic!("rhs is not Mul: {:?}", k),
            },
            k => panic!("not Add: {:?}", k),
        }
    }

    #[test]
    fn stage_with_states() {
        let src = r#"module M {
  stage s(x: Bit(4)) {
    state s1 par {
      _display("s1")
      goto s2
    }
    state s2 par {
      finish
    }
  }
}
"#;
        let unit = parse_ok(src);
        assert_eq!(unit.items.len(), 1);
    }

    #[test]
    fn add4_sample() {
        let src = include_str!("../../../fsl-sample/fsl_tutorial_samples-main/add4.fsl");
        let (result, lex_errs) = parse_source(src);
        assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
        assert!(
            result.errors.is_empty(),
            "parse errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn helloworld_sample() {
        let src = include_str!("../../../fsl-sample/fsl_tutorial_samples-main/HelloWorld.fsl");
        let (result, lex_errs) = parse_source(src);
        assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
        assert!(
            result.errors.is_empty(),
            "parse errors: {:?}",
            result.errors
        );
    }

    macro_rules! sample_test {
        ($name:ident, $path:literal) => {
            #[test]
            fn $name() {
                let src = include_str!($path);
                let (result, lex_errs) = parse_source(src);
                assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
                assert!(
                    result.errors.is_empty(),
                    "parse errors in {}: {:?}",
                    $path,
                    result.errors
                );
            }
        };
    }

    sample_test!(
        s_fun,
        "../../../fsl-sample/fsl_tutorial_samples-main/Fun.fsl"
    );
    sample_test!(
        s_seq,
        "../../../fsl-sample/fsl_tutorial_samples-main/Seq.fsl"
    );
    sample_test!(
        s_stage1,
        "../../../fsl-sample/fsl_tutorial_samples-main/Stage1.fsl"
    );
    sample_test!(
        s_state1,
        "../../../fsl-sample/fsl_tutorial_samples-main/State1.fsl"
    );
    sample_test!(
        s_add4,
        "../../../fsl-sample/fsl_tutorial_samples-main/add4.fsl"
    );
    sample_test!(
        s_add8,
        "../../../fsl-sample/fsl_tutorial_samples-main/add8.fsl"
    );
    sample_test!(
        s_cpu8,
        "../../../fsl-sample/fsl_tutorial_samples-main/cpu8.fsl"
    );
    sample_test!(s_alu32, "../../../fsl-sample/alu32-main/alu32.fsl");
    sample_test!(s_add32, "../../../fsl-sample/alu32-main/add32.fsl");
    sample_test!(s_top_alu32, "../../../fsl-sample/alu32-main/top_alu32.fsl");
    sample_test!(
        s_test_alu32,
        "../../../fsl-sample/alu32-main/test_alu32.fsl"
    );
    sample_test!(s_mult32, "../../../fsl-sample/mult32-main/mult32.fsl");
    sample_test!(
        s_test_mult32,
        "../../../fsl-sample/mult32-main/test_mult32.fsl"
    );
}
