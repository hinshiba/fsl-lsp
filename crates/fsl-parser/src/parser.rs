//! FSL の構文解析器
//!
//! 手書き再帰下降パーサで実装する．
//! 計画では chumsky 0.12 の使用を想定しているが，
//! チュートリアル例を確実に処理することを優先し，まずは手書きで実装する．
//! 後続フェーズで chumsky への置換を検討する．

use crate::ast::*;
use fsl_lexer::{Span, Token};

/// 構文エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// 構文解析結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub unit: CompilationUnit,
    pub errors: Vec<ParseError>,
}

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    errors: Vec<ParseError>,
    src_end: usize,
}

/// ソース末尾位置．エラー報告のフォールバックに用いる．
fn end_span(end: usize) -> Span {
    end..end
}

pub fn parse(tokens: Vec<(Token, Span)>, src_end: usize) -> ParseResult {
    let mut p = Parser {
        tokens,
        pos: 0,
        errors: Vec::new(),
        src_end,
    };
    let unit = p.parse_compilation_unit();
    ParseResult {
        unit,
        errors: p.errors,
    }
}

impl Parser {
    // ----- 基本ヘルパ -----

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| end_span(self.src_end))
    }

    fn previous_span(&self) -> Span {
        if self.pos == 0 {
            end_span(0)
        } else {
            self.tokens[self.pos - 1].1.clone()
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn advance(&mut self) -> Option<(Token, Span)> {
        if self.at_end() {
            None
        } else {
            let r = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(r)
        }
    }

    fn check(&self, kind: &Token) -> bool {
        matches!(self.peek(), Some(t) if std::mem::discriminant(t) == std::mem::discriminant(kind))
    }

    fn eat(&mut self, kind: &Token) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &Token, what: &str) -> Option<Span> {
        if self.check(kind) {
            let span = self.current_span();
            self.advance();
            Some(span)
        } else {
            let span = self.current_span();
            self.errors.push(ParseError {
                message: format!("expected {}, got {:?}", what, self.peek()),
                span,
            });
            None
        }
    }

    fn expect_ident(&mut self, what: &str) -> Option<Ident> {
        match self.peek() {
            Some(Token::Ident(_)) => {
                let (tok, span) = self.advance().unwrap();
                if let Token::Ident(s) = tok {
                    Some(Spanned::new(s, span))
                } else {
                    unreachable!()
                }
            }
            _ => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected {}, got {:?}", what, self.peek()),
                    span,
                });
                None
            }
        }
    }

    /// 同期点までトークンを読み飛ばす
    fn synchronize_to_top(&mut self) {
        while let Some(t) = self.peek() {
            if matches!(t, Token::Module | Token::Trait) {
                break;
            }
            self.advance();
        }
    }

    /// ブロック内の同期．次の `}` あるいは文の頭らしきトークンへ進む．
    fn synchronize_in_block(&mut self) {
        let mut depth = 0;
        while let Some(t) = self.peek() {
            match t {
                Token::LBrace => depth += 1,
                Token::RBrace if depth == 0 => return,
                Token::RBrace => depth -= 1,
                Token::Semicolon if depth == 0 => {
                    self.advance();
                    return;
                }
                _ => {}
            }
            self.advance();
        }
    }

    // ----- トップレベル -----

    fn parse_compilation_unit(&mut self) -> CompilationUnit {
        let mut items = Vec::new();
        while !self.at_end() {
            // セミコロンや余分な区切りを許容
            if self.eat(&Token::Semicolon) {
                continue;
            }
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.synchronize_to_top(),
            }
        }
        CompilationUnit { items }
    }

    fn parse_item(&mut self) -> Option<Item> {
        match self.peek()? {
            Token::Module => self.parse_module().map(Item::Module),
            Token::Trait => self.parse_trait().map(Item::Trait),
            _ => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected item, got {:?}", self.peek()),
                    span,
                });
                self.advance();
                None
            }
        }
    }

    fn parse_module(&mut self) -> Option<ModuleDef> {
        let start = self.current_span().start;
        self.expect(&Token::Module, "`module`")?;
        let name = self.expect_ident("module name")?;

        let extends = if self.eat(&Token::Extends) {
            Some(self.expect_ident("trait or module name")?)
        } else {
            None
        };

        let mut with_traits = Vec::new();
        while self.eat(&Token::With) {
            if let Some(id) = self.expect_ident("trait name") {
                with_traits.push(id);
            }
        }

        self.expect(&Token::LBrace, "`{`")?;
        let items = self.parse_module_items_until_brace();
        let end = self.current_span().end;
        self.expect(&Token::RBrace, "`}`");

        Some(ModuleDef {
            name,
            extends,
            with_traits,
            items,
            span: start..end,
        })
    }

    fn parse_trait(&mut self) -> Option<TraitDef> {
        let start = self.current_span().start;
        self.expect(&Token::Trait, "`trait`")?;
        let name = self.expect_ident("trait name")?;
        self.expect(&Token::LBrace, "`{`")?;
        let items = self.parse_module_items_until_brace();
        let end = self.current_span().end;
        self.expect(&Token::RBrace, "`}`");
        Some(TraitDef {
            name,
            items,
            span: start..end,
        })
    }

    fn parse_module_items_until_brace(&mut self) -> Vec<ModuleItem> {
        let mut items = Vec::new();
        while !self.at_end() && !self.check(&Token::RBrace) {
            if self.eat(&Token::Semicolon) {
                continue;
            }
            let start_pos = self.pos;
            match self.parse_module_item() {
                Some(item) => items.push(item),
                None => {
                    // 進行を保証．自己同期．
                    if self.pos == start_pos {
                        self.advance();
                    }
                    self.synchronize_in_block();
                }
            }
        }
        items
    }

    fn parse_module_item(&mut self) -> Option<ModuleItem> {
        match self.peek()? {
            Token::Reg => self.parse_reg_decl().map(ModuleItem::Reg),
            Token::Mem => self.parse_mem_decl().map(ModuleItem::Mem),
            Token::Input => self
                .parse_port_decl(true)
                .map(|p| ModuleItem::Input(p)),
            Token::Output => self.parse_output_item(),
            Token::Private | Token::Def => self.parse_fn_def().map(ModuleItem::Fn),
            Token::Always => {
                let start = self.current_span().start;
                self.advance();
                let body = self.parse_block()?;
                Some(ModuleItem::Always(Block {
                    span: start..body.span.end,
                    ..body
                }))
            }
            Token::Initial => {
                let start = self.current_span().start;
                self.advance();
                let body = self.parse_block()?;
                Some(ModuleItem::Initial(Block {
                    span: start..body.span.end,
                    ..body
                }))
            }
            Token::Stage => self.parse_stage_def().map(ModuleItem::Stage),
            Token::Type => self.parse_type_def().map(ModuleItem::Type),
            Token::Val => self.parse_val_or_instance(),
            t => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected module item, got {:?}", t),
                    span: span.clone(),
                });
                Some(ModuleItem::Error(span))
            }
        }
    }

    fn parse_reg_decl(&mut self) -> Option<RegDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Reg, "`reg`")?;
        let name = self.expect_ident("register name")?;
        self.expect(&Token::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let init = if self.eat(&Token::Eq) {
            self.parse_expr()
        } else {
            None
        };
        let end = self.previous_span().end;
        Some(RegDecl {
            name,
            ty,
            init,
            span: start..end,
        })
    }

    fn parse_mem_decl(&mut self) -> Option<MemDecl> {
        // mem [<Type>] <name>(<size>) [= (e, e, ...)]
        let start = self.current_span().start;
        self.expect(&Token::Mem, "`mem`")?;
        self.expect(&Token::LBracket, "`[`")?;
        let elem_ty = self.parse_type()?;
        self.expect(&Token::RBracket, "`]`")?;
        let name = self.expect_ident("memory name")?;
        self.expect(&Token::LParen, "`(`")?;
        let size = self.parse_expr()?;
        self.expect(&Token::RParen, "`)`")?;
        let init = if self.eat(&Token::Eq) {
            self.expect(&Token::LParen, "`(`")?;
            let mut elements = Vec::new();
            if !self.check(&Token::RParen) {
                loop {
                    if let Some(e) = self.parse_expr() {
                        elements.push(e);
                    } else {
                        break;
                    }
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen, "`)`");
            elements
        } else {
            Vec::new()
        };
        let end = self.previous_span().end;
        Some(MemDecl {
            name,
            elem_ty,
            size,
            init,
            span: start..end,
        })
    }

    fn parse_port_decl(&mut self, is_input: bool) -> Option<PortDecl> {
        let start = self.current_span().start;
        if is_input {
            self.expect(&Token::Input, "`input`")?;
        } else {
            // 既に `output` を消費済みの呼び出し元がある場合があるため
            // 呼び出し元のロジックで使い分ける．ここでは出力ポート用の入口.
            self.expect(&Token::Output, "`output`")?;
        }
        let name = self.expect_ident("port name")?;
        self.expect(&Token::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let end = self.previous_span().end;
        Some(PortDecl {
            name,
            ty,
            span: start..end,
        })
    }

    /// `output ...` のディスパッチ．`output def ...` か `output <name>: <Type>` を判別する．
    fn parse_output_item(&mut self) -> Option<ModuleItem> {
        let start = self.current_span().start;
        self.expect(&Token::Output, "`output`")?;
        if self.check(&Token::Def) {
            self.advance(); // def
            // optional 受信者は output def では使われないとみなす
            let name = self.expect_ident("function name")?;
            let params = self.parse_params()?;
            self.expect(&Token::Colon, "`:`")?;
            let ret = self.parse_type()?;
            let end = self.previous_span().end;
            Some(ModuleItem::OutputFn(OutputFnDecl {
                name,
                params,
                ret,
                span: start..end,
            }))
        } else {
            let name = self.expect_ident("port name")?;
            self.expect(&Token::Colon, "`:`")?;
            let ty = self.parse_type()?;
            let end = self.previous_span().end;
            Some(ModuleItem::Output(PortDecl {
                name,
                ty,
                span: start..end,
            }))
        }
    }

    fn parse_fn_def(&mut self) -> Option<FnDef> {
        let start = self.current_span().start;
        let is_private = self.eat(&Token::Private);
        self.expect(&Token::Def, "`def`")?;
        // 受信者付きの定義 `def cpu.mem_read(...)` をサポート
        let first = self.expect_ident("function name")?;
        let (receiver, name) = if self.eat(&Token::Dot) {
            let real = self.expect_ident("method name")?;
            (Some(first), real)
        } else {
            (None, first)
        };
        let params = self.parse_params()?;
        let ret = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // body kind 判定:
        //   `=` の後にブロックまたは式  -> Expr
        //   `seq { ... }`                -> Seq
        //   `par { ... }`                -> Par
        let (body_kind, body) = if self.eat(&Token::Eq) {
            let body = self.parse_block_or_expr_block()?;
            (FnBodyKind::Expr, body)
        } else if self.eat(&Token::Seq) {
            let body = self.parse_block()?;
            (FnBodyKind::Seq, body)
        } else if self.eat(&Token::Par) {
            let body = self.parse_block()?;
            (FnBodyKind::Par, body)
        } else {
            // 本体省略は認めない
            let span = self.current_span();
            self.errors.push(ParseError {
                message: "expected function body (`= ...`, `seq { ... }`, or `par { ... }`)"
                    .to_string(),
                span,
            });
            return None;
        };

        let end = self.previous_span().end;
        Some(FnDef {
            is_private,
            receiver,
            name,
            params,
            ret,
            body_kind,
            body,
            span: start..end,
        })
    }

    /// `def f(...) = expr` における式形式．波括弧なしの式も受け入れる．
    fn parse_block_or_expr_block(&mut self) -> Option<Block> {
        if self.check(&Token::LBrace) {
            self.parse_block()
        } else {
            // 単一式を1要素のブロックに包む
            let expr = self.parse_expr()?;
            let span = expr.span.clone();
            Some(Block {
                stmts: vec![Stmt {
                    kind: StmtKind::Expr(expr),
                    span: span.clone(),
                }],
                span,
            })
        }
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        self.expect(&Token::LParen, "`(`")?;
        let mut params = Vec::new();
        if !self.check(&Token::RParen) {
            loop {
                if let Some(name) = self.expect_ident("parameter name") {
                    let ty = if self.eat(&Token::Colon) {
                        self.parse_type()
                    } else {
                        None
                    };
                    params.push(Param { name, ty });
                }
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen, "`)`")?;
        Some(params)
    }

    fn parse_stage_def(&mut self) -> Option<StageDef> {
        let start = self.current_span().start;
        self.expect(&Token::Stage, "`stage`")?;
        let name = self.expect_ident("stage name")?;
        let params = self.parse_params()?;
        self.expect(&Token::LBrace, "`{`")?;
        let mut body = Vec::new();
        while !self.at_end() && !self.check(&Token::RBrace) {
            if self.eat(&Token::Semicolon) {
                continue;
            }
            let pos_before = self.pos;
            let item = match self.peek() {
                Some(Token::State) => self.parse_state_def().map(StageItem::State),
                Some(Token::Reg) => self.parse_reg_decl().map(StageItem::Reg),
                Some(Token::Mem) => self.parse_mem_decl().map(StageItem::Mem),
                Some(Token::Val) => self.parse_val_stmt().map(StageItem::Val),
                _ => self.parse_stmt().map(StageItem::Stmt),
            };
            match item {
                Some(it) => body.push(it),
                None => {
                    if self.pos == pos_before {
                        self.advance();
                    }
                    self.synchronize_in_block();
                }
            }
        }
        let end = self.current_span().end;
        self.expect(&Token::RBrace, "`}`");
        Some(StageDef {
            name,
            params,
            body,
            span: start..end,
        })
    }

    fn parse_state_def(&mut self) -> Option<StateDef> {
        let start = self.current_span().start;
        self.expect(&Token::State, "`state`")?;
        let name = self.expect_ident("state name")?;
        let body = self.parse_stmt()?;
        let end = body.span.end;
        Some(StateDef {
            name,
            body,
            span: start..end,
        })
    }

    fn parse_type_def(&mut self) -> Option<TypeDef> {
        let start = self.current_span().start;
        self.expect(&Token::Type, "`type`")?;
        let name = self.expect_ident("type name")?;
        self.expect(&Token::LParen, "`(`")?;
        let mut fields = Vec::new();
        if !self.check(&Token::RParen) {
            loop {
                let fname = self.expect_ident("field name")?;
                self.expect(&Token::Colon, "`:`")?;
                let fty = self.parse_type()?;
                fields.push(TypeField { name: fname, ty: fty });
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen, "`)`")?;
        let end = self.previous_span().end;
        Some(TypeDef {
            name,
            fields,
            span: start..end,
        })
    }

    /// `val name = new Mod` か `val ... = expr` か判別する
    fn parse_val_or_instance(&mut self) -> Option<ModuleItem> {
        let start = self.current_span().start;
        self.expect(&Token::Val, "`val`")?;

        // パターン部の解析
        let pattern = if self.eat(&Token::LParen) {
            let mut names = Vec::new();
            if !self.check(&Token::RParen) {
                loop {
                    if let Some(n) = self.expect_ident("identifier") {
                        names.push(n);
                    }
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen, "`)`")?;
            ValPattern::Tuple(names)
        } else {
            let n = self.expect_ident("identifier")?;
            ValPattern::Single(n)
        };

        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&Token::Eq, "`=`")?;

        // `new ModName` 即値はモジュールスコープではインスタンス宣言にする
        if self.check(&Token::New) {
            if let ValPattern::Single(name) = pattern {
                self.advance(); // new
                let module_name = self.expect_ident("module name")?;
                let end = self.previous_span().end;
                return Some(ModuleItem::Instance(InstanceDecl {
                    name,
                    module_name,
                    span: start..end,
                }));
            }
        }

        let init = self.parse_expr()?;
        let end = init.span.end;
        Some(ModuleItem::Val(ValDecl {
            pattern,
            ty,
            init,
            span: start..end,
        }))
    }

    // ----- 型 -----

    fn parse_type(&mut self) -> Option<Type> {
        let start = self.current_span().start;
        let kind = match self.peek()? {
            Token::Ident(name) => {
                let name = name.clone();
                let kind_span = self.current_span();
                match name.as_str() {
                    "Unit" => {
                        self.advance();
                        TypeKind::Unit
                    }
                    "Boolean" => {
                        self.advance();
                        TypeKind::Boolean
                    }
                    "Int" => {
                        self.advance();
                        TypeKind::Int
                    }
                    "String" => {
                        self.advance();
                        TypeKind::String
                    }
                    "Bit" => {
                        self.advance();
                        self.expect(&Token::LParen, "`(`")?;
                        let n = self.parse_expr()?;
                        self.expect(&Token::RParen, "`)`")?;
                        TypeKind::Bit(Box::new(n))
                    }
                    "Array" => {
                        self.advance();
                        self.expect(&Token::LBracket, "`[`")?;
                        let inner = self.parse_type()?;
                        self.expect(&Token::RBracket, "`]`")?;
                        TypeKind::Array(Box::new(inner))
                    }
                    "List" => {
                        self.advance();
                        self.expect(&Token::LBracket, "`[`")?;
                        let inner = self.parse_type()?;
                        self.expect(&Token::RBracket, "`]`")?;
                        TypeKind::List(Box::new(inner))
                    }
                    _ => {
                        self.advance();
                        TypeKind::Named(Spanned::new(name, kind_span))
                    }
                }
            }
            Token::LParen => {
                self.advance();
                let mut tys = Vec::new();
                if !self.check(&Token::RParen) {
                    loop {
                        let t = self.parse_type()?;
                        tys.push(t);
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen, "`)`")?;
                TypeKind::Tuple(tys)
            }
            other => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected type, got {:?}", other),
                    span: span.clone(),
                });
                self.advance();
                return Some(Type {
                    kind: TypeKind::Named(Spanned::new("<error>".into(), span.clone())),
                    span,
                });
            }
        };
        let end = self.previous_span().end;
        Some(Type {
            kind,
            span: start..end,
        })
    }

    // ----- 文 -----

    fn parse_block(&mut self) -> Option<Block> {
        let start = self.current_span().start;
        self.expect(&Token::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at_end() && !self.check(&Token::RBrace) {
            if self.eat(&Token::Semicolon) {
                continue;
            }
            let pos_before = self.pos;
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => {
                    if self.pos == pos_before {
                        self.advance();
                    }
                    self.synchronize_in_block();
                }
            }
        }
        let end = self.current_span().end;
        self.expect(&Token::RBrace, "`}`");
        Some(Block {
            stmts,
            span: start..end,
        })
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        match self.peek()? {
            Token::Val => {
                let item = self.parse_val_stmt()?;
                let end = item.span.end;
                Some(Stmt {
                    kind: StmtKind::Val(item),
                    span: start..end,
                })
            }
            Token::Par | Token::Seq | Token::Any | Token::Alt => {
                let kind = match self.peek().unwrap() {
                    Token::Par => BlockKind::Par,
                    Token::Seq => BlockKind::Seq,
                    Token::Any => BlockKind::Any,
                    Token::Alt => BlockKind::Alt,
                    _ => unreachable!(),
                };
                self.advance();
                let block = self.parse_block()?;
                let end = block.span.end;
                Some(Stmt {
                    kind: StmtKind::BlockKind(kind, block),
                    span: start..end,
                })
            }
            Token::Generate => {
                self.advance();
                let target = self.expect_ident("stage name")?;
                self.expect(&Token::LParen, "`(`")?;
                let args = self.parse_arg_list()?;
                self.expect(&Token::RParen, "`)`")?;
                let end = self.previous_span().end;
                Some(Stmt {
                    kind: StmtKind::Generate(target, args),
                    span: start..end,
                })
            }
            Token::Relay => {
                self.advance();
                let target = self.expect_ident("stage name")?;
                self.expect(&Token::LParen, "`(`")?;
                let args = self.parse_arg_list()?;
                self.expect(&Token::RParen, "`)`")?;
                let end = self.previous_span().end;
                Some(Stmt {
                    kind: StmtKind::Relay(target, args),
                    span: start..end,
                })
            }
            Token::Finish => {
                self.advance();
                // `finish` は引数を取る場合 (`_finish` ではなく `finish` のみ) と取らない場合がある．
                // ここではキーワード版は引数なしのみ受理．`_finish(...)` は識別子として扱う．
                let end = self.previous_span().end;
                Some(Stmt {
                    kind: StmtKind::Finish,
                    span: start..end,
                })
            }
            Token::Goto => {
                self.advance();
                let target = self.expect_ident("state name")?;
                let end = self.previous_span().end;
                Some(Stmt {
                    kind: StmtKind::Goto(target),
                    span: start..end,
                })
            }
            _ => {
                // 式または代入の文
                let lhs = self.parse_expr()?;
                if self.eat(&Token::ColonEq) {
                    let rhs = self.parse_expr()?;
                    let end = rhs.span.end;
                    return Some(Stmt {
                        kind: StmtKind::RegAssign(lhs, rhs),
                        span: start..end,
                    });
                }
                if self.eat(&Token::Eq) {
                    let rhs = self.parse_expr()?;
                    let end = rhs.span.end;
                    return Some(Stmt {
                        kind: StmtKind::Assign(lhs, rhs),
                        span: start..end,
                    });
                }
                let end = lhs.span.end;
                Some(Stmt {
                    kind: StmtKind::Expr(lhs),
                    span: start..end,
                })
            }
        }
    }

    fn parse_val_stmt(&mut self) -> Option<ValDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Val, "`val`")?;
        let pattern = if self.eat(&Token::LParen) {
            let mut names = Vec::new();
            if !self.check(&Token::RParen) {
                loop {
                    if let Some(n) = self.expect_ident("identifier") {
                        names.push(n);
                    }
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen, "`)`")?;
            ValPattern::Tuple(names)
        } else {
            let n = self.expect_ident("identifier")?;
            ValPattern::Single(n)
        };
        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Eq, "`=`")?;
        let init = self.parse_expr()?;
        let end = init.span.end;
        Some(ValDecl {
            pattern,
            ty,
            init,
            span: start..end,
        })
    }

    fn parse_arg_list(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if !self.check(&Token::RParen) {
            loop {
                let e = self.parse_expr()?;
                args.push(e);
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
        }
        Some(args)
    }

    // ----- 式（演算子優先順位） -----

    fn parse_expr(&mut self) -> Option<Expr> {
        let mut expr = self.parse_or()?;
        // 後置 `match { case ... }` をフォールドする
        while self.check(&Token::Match) {
            self.advance();
            let arms = self.parse_match_arms()?;
            let start = expr.span.start;
            let end = self.previous_span().end;
            expr = Expr {
                kind: ExprKind::Match(Box::new(expr), arms),
                span: start..end,
            };
        }
        Some(expr)
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_and()?;
        while self.check(&Token::PipePipe) {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Self::mk_bin(BinaryOp::LogOr, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_cmp()?;
        while self.check(&Token::AmpAmp) {
            self.advance();
            let rhs = self.parse_cmp()?;
            lhs = Self::mk_bin(BinaryOp::LogAnd, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_cmp(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_bitor_xor()?;
        loop {
            let op = match self.peek() {
                Some(Token::EqEq) => BinaryOp::Eq,
                Some(Token::NotEq) => BinaryOp::Ne,
                Some(Token::Lt) => BinaryOp::Lt,
                Some(Token::Le) => BinaryOp::Le,
                Some(Token::Gt) => BinaryOp::Gt,
                Some(Token::Ge) => BinaryOp::Ge,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_bitor_xor()?;
            lhs = Self::mk_bin(op, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_bitor_xor(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_bitand()?;
        loop {
            let op = match self.peek() {
                Some(Token::Pipe) => BinaryOp::BitOr,
                Some(Token::Caret) => BinaryOp::BitXor,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_bitand()?;
            lhs = Self::mk_bin(op, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_bitand(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_shift()?;
        while self.check(&Token::Amp) {
            self.advance();
            let rhs = self.parse_shift()?;
            lhs = Self::mk_bin(BinaryOp::BitAnd, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_shift(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_concat()?;
        loop {
            let op = match self.peek() {
                Some(Token::Shl) => BinaryOp::Shl,
                Some(Token::Shr) => BinaryOp::Shr,
                Some(Token::ShrLogical) => BinaryOp::ShrLogical,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_concat()?;
            lhs = Self::mk_bin(op, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_concat(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_addsub()?;
        while self.check(&Token::PlusPlus) {
            self.advance();
            let rhs = self.parse_addsub()?;
            lhs = Self::mk_bin(BinaryOp::Concat, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_addsub(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Self::mk_bin(op, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_mul(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_signext()?;
        while self.check(&Token::Star) {
            self.advance();
            let rhs = self.parse_signext()?;
            lhs = Self::mk_bin(BinaryOp::Mul, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_signext(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;
        while self.check(&Token::Hash) {
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Self::mk_bin(BinaryOp::SignExt, lhs, rhs);
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        match self.peek() {
            Some(Token::Tilde) => {
                self.advance();
                let inner = self.parse_unary()?;
                let end = inner.span.end;
                Some(Expr {
                    kind: ExprKind::Unary(UnaryOp::BitNot, Box::new(inner)),
                    span: start..end,
                })
            }
            Some(Token::Bang) => {
                self.advance();
                let inner = self.parse_unary()?;
                let end = inner.span.end;
                Some(Expr {
                    kind: ExprKind::Unary(UnaryOp::LogNot, Box::new(inner)),
                    span: start..end,
                })
            }
            Some(Token::Minus) => {
                self.advance();
                let inner = self.parse_unary()?;
                let end = inner.span.end;
                Some(Expr {
                    kind: ExprKind::Unary(UnaryOp::Neg, Box::new(inner)),
                    span: start..end,
                })
            }
            // リダクションOR `|x` は括弧内に出現する慣習なのでここでも受ける
            Some(Token::Pipe) => {
                self.advance();
                let inner = self.parse_unary()?;
                let end = inner.span.end;
                Some(Expr {
                    kind: ExprKind::Unary(UnaryOp::RedOr, Box::new(inner)),
                    span: start..end,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.advance();
                    let name = self.expect_ident("field or method name")?;
                    let span_start = expr.span.start;
                    let end = name.span.end;
                    let new_expr = Expr {
                        kind: ExprKind::Field(Box::new(expr), name),
                        span: span_start..end,
                    };
                    expr = new_expr;
                }
                Some(Token::LParen) => {
                    self.advance();
                    let args = self.parse_arg_list()?;
                    let close = self.expect(&Token::RParen, "`)`")?;
                    let span_start = expr.span.start;
                    expr = Expr {
                        kind: ExprKind::Call(Box::new(expr), args),
                        span: span_start..close.end,
                    };
                }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        match self.peek()? {
            Token::IntLit(_) => {
                let (tok, span) = self.advance().unwrap();
                let s = if let Token::IntLit(s) = tok { s } else { unreachable!() };
                Some(Expr {
                    kind: ExprKind::Int(s),
                    span,
                })
            }
            Token::StringLit(_) => {
                let (tok, span) = self.advance().unwrap();
                let s = if let Token::StringLit(s) = tok { s } else { unreachable!() };
                Some(Expr {
                    kind: ExprKind::Str(s),
                    span,
                })
            }
            Token::True => {
                let span = self.current_span();
                self.advance();
                Some(Expr {
                    kind: ExprKind::Bool(true),
                    span,
                })
            }
            Token::False => {
                let span = self.current_span();
                self.advance();
                Some(Expr {
                    kind: ExprKind::Bool(false),
                    span,
                })
            }
            Token::Ident(_) => {
                let (tok, span) = self.advance().unwrap();
                let s = if let Token::Ident(s) = tok { s } else { unreachable!() };
                Some(Expr {
                    kind: ExprKind::Path(Spanned::new(s, span.clone())),
                    span,
                })
            }
            Token::LParen => {
                self.advance();
                if self.eat(&Token::RParen) {
                    let end = self.previous_span().end;
                    return Some(Expr {
                        kind: ExprKind::Unit,
                        span: start..end,
                    });
                }
                let first = self.parse_expr()?;
                if self.eat(&Token::Comma) {
                    let mut elems = vec![first];
                    if !self.check(&Token::RParen) {
                        loop {
                            let e = self.parse_expr()?;
                            elems.push(e);
                            if !self.eat(&Token::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(&Token::RParen, "`)`")?;
                    let end = self.previous_span().end;
                    return Some(Expr {
                        kind: ExprKind::Tuple(elems),
                        span: start..end,
                    });
                }
                self.expect(&Token::RParen, "`)`")?;
                Some(first)
            }
            Token::LBrace => {
                let block = self.parse_block()?;
                let span = block.span.clone();
                Some(Expr {
                    kind: ExprKind::Block(block),
                    span,
                })
            }
            Token::If => self.parse_if(),
            Token::Match => self.parse_match_kw_alone(),
            Token::New => {
                self.advance();
                let name = self.expect_ident("module name")?;
                let end = name.span.end;
                Some(Expr {
                    kind: ExprKind::New(name),
                    span: start..end,
                })
            }
            other => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected expression, got {:?}", other),
                    span: span.clone(),
                });
                self.advance();
                Some(Expr {
                    kind: ExprKind::Error,
                    span,
                })
            }
        }
    }

    fn parse_if(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(&Token::If, "`if`")?;
        self.expect(&Token::LParen, "`(`")?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen, "`)`")?;
        // then 部分は単独式または文ブロック等
        let then = self.parse_then_branch()?;
        let else_ = if self.eat(&Token::Else) {
            Some(Box::new(self.parse_then_branch()?))
        } else {
            None
        };
        let end = else_
            .as_ref()
            .map(|e| e.span.end)
            .unwrap_or(then.span.end);
        Some(Expr {
            kind: ExprKind::If(Box::new(cond), Box::new(then), else_),
            span: start..end,
        })
    }

    /// `if` の分岐部．文相当のキーワード文と式の両方を受ける．
    fn parse_then_branch(&mut self) -> Option<Expr> {
        if self.check(&Token::LBrace) {
            let block = self.parse_block()?;
            let span = block.span.clone();
            return Some(Expr {
                kind: ExprKind::Block(block),
                span,
            });
        }
        if matches!(
            self.peek(),
            Some(Token::Par | Token::Seq | Token::Any | Token::Alt | Token::Generate
                | Token::Relay | Token::Finish | Token::Goto)
        ) {
            // 文相当をブロックに包む
            let stmt = self.parse_stmt()?;
            let span = stmt.span.clone();
            return Some(Expr {
                kind: ExprKind::Block(Block {
                    stmts: vec![stmt],
                    span: span.clone(),
                }),
                span,
            });
        }
        self.parse_expr()
    }

    /// `match` 単独形式（`expr match` ではなく `match expr` 形式の予約）．
    /// 実際には postfix 連鎖で `expr match { ... }` を扱う必要があるため，
    /// 後置で受ける別経路をオプションで用意する余地を残す．
    fn parse_match_kw_alone(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(&Token::Match, "`match`")?;
        // この経路は Reserve．
        let _ = start;
        let span = self.previous_span();
        self.errors.push(ParseError {
            message: "bare `match` is not supported; use `expr match { ... }`".into(),
            span: span.clone(),
        });
        Some(Expr {
            kind: ExprKind::Error,
            span,
        })
    }

    /// `expr match { case p => e ... }` の解析
    /// 注: `match` は postfix 演算子レベルで現れるため，現状の優先順位では
    /// 構文中に現れるのは限定的．ここでは parse_expr 後の追加処理として呼ぶ．
    fn parse_match_arms(&mut self) -> Option<Vec<MatchArm>> {
        self.expect(&Token::LBrace, "`{`")?;
        let mut arms = Vec::new();
        while !self.at_end() && !self.check(&Token::RBrace) {
            if self.eat(&Token::Semicolon) {
                continue;
            }
            let arm_start = self.current_span().start;
            self.expect(&Token::Case, "`case`")?;
            let pat = self.parse_pattern()?;
            self.expect(&Token::FatArrow, "`=>`")?;
            // ブロック相当（par/seq/any/alt や `{ ... }`）も受け入れる
            let body = self.parse_then_branch()?;
            let end = body.span.end;
            arms.push(MatchArm {
                pattern: pat,
                body,
                span: arm_start..end,
            });
        }
        self.expect(&Token::RBrace, "`}`")?;
        Some(arms)
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        match self.peek()? {
            Token::Ident(s) if s == "_" => {
                self.advance();
                Some(Pattern::Wildcard)
            }
            Token::Ident(_) => {
                let (tok, span) = self.advance().unwrap();
                let s = if let Token::Ident(s) = tok { s } else { unreachable!() };
                Some(Pattern::Ident(Spanned::new(s, span)))
            }
            Token::IntLit(_) => {
                let (tok, _) = self.advance().unwrap();
                let s = if let Token::IntLit(s) = tok { s } else { unreachable!() };
                Some(Pattern::IntLit(s))
            }
            other => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    message: format!("expected pattern, got {:?}", other),
                    span,
                });
                self.advance();
                None
            }
        }
    }

    fn mk_bin(op: BinaryOp, l: Expr, r: Expr) -> Expr {
        let span = l.span.start..r.span.end;
        Expr {
            kind: ExprKind::Binary(op, Box::new(l), Box::new(r)),
            span,
        }
    }
}

