

  infix(left(8), just(Token::Plus),
      |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Add, l, r, e.span())),
  infix(left(8), just(Token::Minus),
      |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Sub, l, r, e.span())),

  ヘルパーで

  fn binop_l(prec: u32, tok: Token, op: BinaryOp) -> impl ... {
      infix(left(prec), just(tok),
          move |l, _, r, e: &mut MapExtra<_,_>| mk_bin(op, l, r, e.span()))
  }
  fn unop(prec: u32, tok: Token, op: UnaryOp) -> impl ... {
      prefix(prec, just(tok),
          move |_, rhs, e: &mut MapExtra<_,_>| Expr {
              kind: ExprKind::Unary(op, Box::new(rhs)), span: e.span() })
  }

  としてテーブルを

  .pratt((
      postfix(12, call_args, |lhs, args, e| spanned(ExprKind::Call(Box::new(lhs), args), e.span())),
      postfix(12, dot_ident, |lhs, n,    e| spanned(ExprKind::Field(Box::new(lhs), n),    e.span())),
      unop(11, Token::Tilde, UnaryOp::BitNot),
      unop(11, Token::Bang,  UnaryOp::LogNot),
      unop(11, Token::Minus, UnaryOp::Neg),
      unop(11, Token::Pipe,  UnaryOp::RedOr),
      binop_l(10, Token::Hash,       BinaryOp::SignExt),
      binop_l(9,  Token::Star,       BinaryOp::Mul),
      binop_l(8,  Token::Plus,       BinaryOp::Add),
      binop_l(8,  Token::Minus,      BinaryOp::Sub),
      // ... 1行ずつ
      binop_l(1,  Token::PipePipe,   BinaryOp::LogOr),
  ))

  200行→30行に圧縮
