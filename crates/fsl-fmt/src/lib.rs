//! FSL ソースコードのフォーマッタ。
//!
//! 公開 API は [`format`] のみ。パース／レキサーエラー時は何も返さない。

use fsl_parser::{
    BinaryOp, Expr, Expr_, Field, FnDef, FslType, FslType_, InputDecl, Item, MemDecl, ModuleDef,
    NewInstance, OutputDecl, Param, Pattern, RegDecl, Spanned, TraitDef, UnaryOp, ValDecl, ValLhs,
};

const INDENT: &str = "  ";

/// FSL ソースを整形した文字列を返す。
///
/// パースまたはレキサーエラーが 1 つでもある場合は `None` を返し、部分整形しない。
/// 成功時は末尾に改行 1 つを含む整形済み文字列を返す（空ソースを除く）。
pub fn format(src: &str) -> Option<String> {
    // パース / レキサーエラーがあれば部分整形しない
    let (result, lex_errs) = fsl_parser::parse(src);
    if !lex_errs.is_empty() || !result.errors.is_empty() {
        return None;
    }

    let mut w = Writer::new(src);
    w.write_unit(&result.unit.items);
    Some(w.finish())
}

/// 整形状態。`out` に書き出しつつ、未出力コメントをスパン順に保持する。
struct Writer<'a> {
    /// 物理行末（次の `\n` 位置）を求めるためにソースを保持
    src: &'a str,
    out: String,
    /// (start_pos, comment_text) を位置順に保持。`/* */` は内部改行を含み得る。
    comments: Vec<(usize, String)>,
    /// 次に出力するコメントのインデックス
    next_comment: usize,
}

impl<'a> Writer<'a> {
    fn new(src: &'a str) -> Self {
        // ソースを直接 lex してコメントだけ収集する（パーサは trivia を捨てているため）
        let lex_result = fsl_lexer::lex(src);
        let comments = lex_result
            .oks
            .into_iter()
            .filter_map(|t| match t.tok {
                fsl_lexer::Token::LineComment(s) | fsl_lexer::Token::BlockComment(s) => {
                    Some((t.span.start, s))
                }
                _ => None,
            })
            .collect();
        Writer {
            src,
            out: String::new(),
            comments,
            next_comment: 0,
        }
    }

    /// `pos` 以降で最初に現れる `\n` の位置（なければソース末尾）。
    /// 物理行末トレイリングコメントを掴むために使用。
    fn line_end_after(&self, pos: usize) -> usize {
        self.src[pos..]
            .find('\n')
            .map(|n| pos + n)
            .unwrap_or(self.src.len())
    }

    /// 文の「インラインコメント取り込み上限」を求める。
    /// 入れ子ブロックがあればその開始位置（内部は内側で処理）、
    /// なければ物理行末（同一行末トレイリングコメントを取り込む）。
    /// ただし呼び出し側のブロック境界を越えないように clamp する。
    fn inline_limit_for_expr(&self, e: &Expr, block_end: usize) -> usize {
        match first_nested_block_start_in_expr(e) {
            Some(p) => p,
            None => self.line_end_after(e.span.end).min(block_end),
        }
    }

    fn inline_limit_for_field(&self, f: &Field, span_end: usize, body_end: usize) -> usize {
        match first_nested_block_start_in_field(f) {
            Some(p) => p,
            None => self.line_end_after(span_end).min(body_end),
        }
    }

    /// 整形を終了し、末尾に残ったコメントを行頭に流し込んで返す。
    fn finish(mut self) -> String {
        self.flush_remaining_comments(0);
        self.out
    }

    /// `pos` より前にあるコメントをすべて行頭に書き出す。
    /// 各コメントは独立行とし、ブロックコメント内の既存改行は維持。
    fn flush_comments_before(&mut self, pos: usize, depth: usize) {
        while self.next_comment < self.comments.len() && self.comments[self.next_comment].0 < pos {
            let (_, text) = &self.comments[self.next_comment].clone();
            self.write_indent(depth);
            self.out.push_str(text);
            self.out.push('\n');
            self.next_comment += 1;
        }
    }

    /// 残った全コメントを書き出す（ファイル末尾）。
    fn flush_remaining_comments(&mut self, depth: usize) {
        while self.next_comment < self.comments.len() {
            let (_, text) = &self.comments[self.next_comment].clone();
            self.write_indent(depth);
            self.out.push_str(text);
            self.out.push('\n');
            self.next_comment += 1;
        }
    }

    fn write_indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
    }

    // ============================================================
    // トップレベル
    // ============================================================

    fn write_unit(&mut self, items: &[Spanned<Item>]) {
        for (i, item) in items.iter().enumerate() {
            // トップレベルではアイテム前のコメントのみ排出（中身は item 自身が depth+1 で処理）
            self.flush_comments_before(item.span.start, 0);
            if i > 0 {
                self.out.push('\n');
            }
            self.write_item(&item.inner, item.span.end, 0);
        }
    }

    fn write_item(&mut self, item: &Item, body_end: usize, depth: usize) {
        match item {
            Item::Module(m) => self.write_module(m, body_end, depth),
            Item::Trait(t) => self.write_trait(t, body_end, depth),
        }
    }

    fn write_module(&mut self, m: &ModuleDef, body_end: usize, depth: usize) {
        self.out.push_str("module ");
        self.out.push_str(&m.name.inner);
        if let Some(parent) = &m.extends {
            self.out.push_str(" extends ");
            self.out.push_str(&parent.inner);
        }
        if let Some(traits) = &m.with_traits {
            for t in traits {
                self.out.push_str(" with ");
                self.out.push_str(&t.inner);
            }
        }
        self.write_fields(&m.items, body_end, depth);
        self.out.push('\n');
    }

    fn write_trait(&mut self, t: &TraitDef, body_end: usize, depth: usize) {
        self.out.push_str("trait ");
        self.out.push_str(&t.name.inner);
        self.write_fields(&t.items, body_end, depth);
        self.out.push('\n');
    }

    /// `{ ... }` フィールドブロックを整形。空かつコメントなしなら ` {}` 単一行。
    /// 末尾コメントはブロック境界を越えず `}` の直前にとどめる。
    fn write_fields(&mut self, items: &[Spanned<Field>], body_end: usize, depth: usize) {
        if items.is_empty() && !self.has_comments_before(body_end) {
            self.out.push_str(" {}");
            return;
        }
        self.out.push_str(" {\n");
        for it in items {
            // フィールド内インラインコメント＋同一行末トレイリングコメントを上に持ち上げる。
            // 入れ子ブロックがある場合はそのブロック開始位置までしか掃かない。
            let limit = self.inline_limit_for_field(&it.inner, it.span.end, body_end);
            self.flush_comments_before(limit, depth + 1);
            self.write_indent(depth + 1);
            self.write_field(&it.inner, depth + 1);
            self.out.push('\n');
        }
        // ブロック内末尾のコメントを `}` の直前に排出
        self.flush_comments_before(body_end, depth + 1);
        self.write_indent(depth);
        self.out.push('}');
    }

    /// `pos` より前に未出力コメントが残っているか。
    fn has_comments_before(&self, pos: usize) -> bool {
        self.next_comment < self.comments.len() && self.comments[self.next_comment].0 < pos
    }

    // ============================================================
    // フィールド
    // ============================================================

    fn write_field(&mut self, f: &Field, depth: usize) {
        match f {
            Field::Val(v) => self.write_val(v, depth),
            Field::Reg(r) => self.write_reg(r, depth),
            Field::Input(i) => self.write_input(i),
            Field::Output(o) => self.write_output(o),
            Field::Always(e) => {
                self.out.push_str("always ");
                self.write_expr(e, depth);
            }
            Field::Initial(e) => {
                self.out.push_str("initial ");
                self.write_expr(e, depth);
            }
            Field::Fn(f) => self.write_fn(f, depth),
            Field::Mem(m) => self.write_mem(m, depth),
            Field::NewInstance(n) => self.write_new_instance(n),
            // TODO: OutputFn, Composite, Stage, Error
            _ => {}
        }
    }

    fn write_input(&mut self, i: &InputDecl) {
        self.out.push_str("input ");
        self.out.push_str(&i.name.inner);
        self.out.push_str(": ");
        self.write_type(&i.ty);
    }

    fn write_output(&mut self, o: &OutputDecl) {
        self.out.push_str("output ");
        self.out.push_str(&o.name.inner);
        self.out.push_str(": ");
        self.write_type(&o.ty);
    }

    fn write_reg(&mut self, r: &RegDecl, depth: usize) {
        self.out.push_str("reg ");
        self.out.push_str(&r.name.inner);
        self.out.push_str(": ");
        self.write_type(&r.ty);
        if let Some(init) = &r.init {
            self.out.push_str(" = ");
            self.write_expr(init, depth);
        }
    }

    fn write_val(&mut self, v: &ValDecl, depth: usize) {
        self.out.push_str("val ");
        match &v.pattern {
            ValLhs::Single(name) => self.out.push_str(&name.inner),
            ValLhs::Tuple(names) => {
                self.out.push('(');
                let parts: Vec<&str> = names.iter().map(|n| n.inner.as_str()).collect();
                self.out.push_str(&parts.join(", "));
                self.out.push(')');
            }
        }
        if let Some(ty) = &v.ty {
            self.out.push_str(": ");
            self.write_type(ty);
        }
        self.out.push_str(" = ");
        self.write_expr(&v.init, depth);
    }

    fn write_mem(&mut self, m: &MemDecl, depth: usize) {
        self.out.push_str("mem [");
        self.write_type(&m.elem_ty);
        self.out.push_str("] ");
        self.out.push_str(&m.name.inner);
        self.out.push('(');
        self.write_expr(&m.size, depth);
        self.out.push(')');
        // TODO: init values
    }

    fn write_new_instance(&mut self, n: &NewInstance) {
        self.out.push_str("val ");
        self.out.push_str(&n.name.inner);
        self.out.push_str(" = new ");
        self.out.push_str(&n.module_name.inner);
    }

    fn write_fn(&mut self, f: &FnDef, depth: usize) {
        if f.is_private {
            self.out.push_str("private ");
        }
        self.out.push_str("def ");
        if let Some(recv) = &f.receiver {
            self.out.push_str(&recv.inner);
            self.out.push('.');
        }
        self.out.push_str(&f.name.inner);
        self.out.push('(');
        self.write_params(&f.params);
        self.out.push(')');
        if let Some(ret) = &f.ret {
            self.out.push_str(": ");
            self.write_type(ret);
        }
        // 関数本体: seq/par ならキーワード付き、それ以外は `= expr`
        match &f.body.inner {
            Expr_::Seq(stmts) => {
                self.out.push_str(" seq ");
                self.write_block(stmts, f.body.span.end, depth);
            }
            Expr_::Par(stmts) => {
                self.out.push_str(" par ");
                self.write_block(stmts, f.body.span.end, depth);
            }
            _ => {
                self.out.push_str(" = ");
                self.write_expr(&f.body, depth);
            }
        }
    }

    fn write_params(&mut self, params: &[Spanned<Param>]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&p.inner.name.inner);
            if let Some(ty) = &p.inner.ty {
                self.out.push_str(": ");
                self.write_type(ty);
            }
        }
    }

    // ============================================================
    // 式
    // ============================================================

    fn write_expr(&mut self, e: &Expr, depth: usize) {
        match &e.inner {
            Expr_::IntLit(n) => self.out.push_str(&n.to_string()),
            Expr_::BitLit(n) => {
                // ビット幅情報は AST に残らないため hex で出力する
                self.out.push_str(&format!("0x{:x}", n));
            }
            Expr_::Variable(name) => self.out.push_str(&name.inner),
            Expr_::Binary(op, lhs, rhs) => {
                let p = binary_prec(*op);
                // 左結合: 左側は同優先度なら括弧不要、右側は同優先度でも括弧必須
                self.write_binary_operand(lhs, p, false, depth);
                self.out.push(' ');
                self.out.push_str(binary_op_str(*op));
                self.out.push(' ');
                self.write_binary_operand(rhs, p, true, depth);
            }
            Expr_::Unary(op, rhs) => {
                self.out.push_str(unary_op_str(*op));
                self.write_expr(rhs, depth);
            }
            Expr_::Call(callee, args) => {
                self.write_expr(callee, depth);
                self.out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.write_expr(arg, depth);
                }
                self.out.push(')');
            }
            Expr_::Block(stmts) => self.write_block(stmts, e.span.end, depth),
            Expr_::Seq(stmts) => {
                self.out.push_str("seq ");
                self.write_block(stmts, e.span.end, depth);
            }
            Expr_::Par(stmts) => {
                self.out.push_str("par ");
                self.write_block(stmts, e.span.end, depth);
            }
            Expr_::MemAssign(lhs, rhs) => {
                self.write_expr(lhs, depth);
                self.out.push_str(" := ");
                self.write_expr(rhs, depth);
            }
            Expr_::PortAssign(lhs, rhs) => {
                self.write_expr(lhs, depth);
                self.out.push_str(" = ");
                self.write_expr(rhs, depth);
            }
            Expr_::If(cond, then, else_opt) => {
                self.out.push_str("if (");
                self.write_expr(cond, depth);
                self.out.push_str(") ");
                self.write_expr(then, depth);
                if let Some(else_) = else_opt {
                    self.out.push_str(" else ");
                    self.write_expr(else_, depth);
                }
            }
            Expr_::StringLit(s) => self.out.push_str(s),
            Expr_::Bool(b) => self.out.push_str(if *b { "true" } else { "false" }),
            Expr_::Tuple(exprs) => {
                self.out.push('(');
                for (i, e) in exprs.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.write_expr(e, depth);
                }
                self.out.push(')');
            }
            Expr_::Field(e, name) => {
                self.write_expr(e, depth);
                self.out.push('.');
                self.out.push_str(&name.inner);
            }
            Expr_::Match(scrutinee, arms) => {
                self.write_expr(scrutinee, depth);
                self.out.push_str(" match {\n");
                for arm in arms {
                    self.flush_comments_before(arm.span.end, depth + 1);
                    self.write_indent(depth + 1);
                    self.out.push_str("case ");
                    write_pattern(&arm.inner.pattern, &mut self.out);
                    self.out.push_str(" => ");
                    self.write_expr(&arm.inner.body, depth + 1);
                    self.out.push('\n');
                }
                // match ブロック末尾のコメントを `}` 直前に排出
                self.flush_comments_before(e.span.end, depth + 1);
                self.write_indent(depth);
                self.out.push('}');
            }
            Expr_::ValDecl(v) => self.write_val(v, depth),
            Expr_::New(name) => {
                self.out.push_str("new ");
                self.out.push_str(&name.inner);
            }
            Expr_::Unit => {}
            // TODO: Any, Alt, Generate, Relay, Finish, Goto, Error
            _ => {}
        }
    }

    /// 二項演算の子オペランドを出力。親の優先度・左右位置に応じて括弧を挿入。
    fn write_binary_operand(&mut self, e: &Expr, parent_prec: u8, is_right: bool, depth: usize) {
        let needs_parens = match &e.inner {
            Expr_::Binary(op, _, _) => {
                let cp = binary_prec(*op);
                if is_right {
                    cp <= parent_prec
                } else {
                    cp < parent_prec
                }
            }
            _ => false,
        };
        if needs_parens {
            self.out.push('(');
            self.write_expr(e, depth);
            self.out.push(')');
        } else {
            self.write_expr(e, depth);
        }
    }

    /// 文の連なりを `{ ... }` ブロックとして整形（K&R）。
    /// 空かつコメントなしなら `{}` 単一行。末尾コメントはブロック内に保持。
    fn write_block(&mut self, stmts: &[Expr], block_end: usize, depth: usize) {
        if stmts.is_empty() && !self.has_comments_before(block_end) {
            self.out.push_str("{}");
            return;
        }
        self.out.push_str("{\n");
        for s in stmts {
            // 文内インラインコメント＋同一行末トレイリングコメントを上に持ち上げる。
            let limit = self.inline_limit_for_expr(s, block_end);
            self.flush_comments_before(limit, depth + 1);
            self.write_indent(depth + 1);
            self.write_expr(s, depth + 1);
            self.out.push('\n');
        }
        // ブロック末尾コメントを `}` 直前に
        self.flush_comments_before(block_end, depth + 1);
        self.write_indent(depth);
        self.out.push('}');
    }

    // ============================================================
    // 型
    // ============================================================

    fn write_type(&mut self, t: &FslType) {
        match &t.inner {
            FslType_::Unit => self.out.push_str("Unit"),
            FslType_::Boolean => self.out.push_str("Boolean"),
            FslType_::Int => self.out.push_str("Int"),
            FslType_::String => self.out.push_str("String"),
            FslType_::Bit(width) => {
                self.out.push_str("Bit(");
                // 型引数の式はインライン (深度 0)
                self.write_expr(width, 0);
                self.out.push(')');
            }
            FslType_::Array(inner) => {
                self.out.push_str("Array[");
                self.write_type(inner);
                self.out.push(']');
            }
            FslType_::List(inner) => {
                self.out.push_str("List[");
                self.write_type(inner);
                self.out.push(']');
            }
            FslType_::Tuple(types) => {
                self.out.push('(');
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.write_type(ty);
                }
                self.out.push(')');
            }
            FslType_::Named(name) => self.out.push_str(&name.inner),
        }
    }
}

// ============================================================
// 自由関数（Writer 状態に依存しない補助）
// ============================================================

fn write_pattern(p: &Pattern, out: &mut String) {
    match p {
        Pattern::Wildcard => out.push('_'),
        Pattern::Ident(name) => out.push_str(&name.inner),
        Pattern::IntLit(n) => out.push_str(&n.to_string()),
        Pattern::BitLit(n) => out.push_str(&format!("0x{:x}", n)),
    }
}

/// `Option<usize>` の最小値を取る。両方 None なら None。
fn min_opt(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// 式中に登場する最初の入れ子ブロック（Block / Seq / Par / Match）の開始位置。
///
/// インラインコメントを「上に持ち上げる」際の上限として使用。これより後ろの
/// コメントは入れ子ブロック側で処理されるべきもの。
fn first_nested_block_start_in_expr(e: &Expr) -> Option<usize> {
    use Expr_::*;
    match &e.inner {
        Block(_) | Seq(_) | Par(_) | Match(_, _) | Any(_, _) | Alt(_, _) => Some(e.span.start),
        Binary(_, lhs, rhs)
        | MemAssign(lhs, rhs)
        | PortAssign(lhs, rhs) => min_opt(
            first_nested_block_start_in_expr(lhs),
            first_nested_block_start_in_expr(rhs),
        ),
        Unary(_, rhs) => first_nested_block_start_in_expr(rhs),
        Call(callee, args) => {
            let c = first_nested_block_start_in_expr(callee);
            let a = args.iter().filter_map(first_nested_block_start_in_expr).min();
            min_opt(c, a)
        }
        If(cond, then_, else_) => {
            let c = first_nested_block_start_in_expr(cond);
            let t = first_nested_block_start_in_expr(then_);
            let el = else_.as_ref().and_then(|x| first_nested_block_start_in_expr(x));
            min_opt(min_opt(c, t), el)
        }
        Tuple(exprs) => exprs.iter().filter_map(first_nested_block_start_in_expr).min(),
        Field(e, _) => first_nested_block_start_in_expr(e),
        ValDecl(v) => first_nested_block_start_in_expr(&v.init),
        Generate(_, args) | Relay(_, args) => {
            args.iter().filter_map(first_nested_block_start_in_expr).min()
        }
        _ => None,
    }
}

/// フィールド中の最初の入れ子ブロック開始位置。
fn first_nested_block_start_in_field(f: &Field) -> Option<usize> {
    match f {
        Field::Val(v) => first_nested_block_start_in_expr(&v.init),
        Field::Reg(r) => r.init.as_ref().and_then(first_nested_block_start_in_expr),
        Field::Mem(m) => first_nested_block_start_in_expr(&m.size),
        Field::Always(e) | Field::Initial(e) => first_nested_block_start_in_expr(e),
        Field::Fn(f) => first_nested_block_start_in_expr(&f.body),
        _ => None,
    }
}

fn unary_op_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::BitNot => "~",
        UnaryOp::ReducAnd => "&",
        UnaryOp::ReducOr => "|",
        UnaryOp::ReducXor => "^",
        UnaryOp::LogNot => "!",
        UnaryOp::Neg => "-",
    }
}

/// 二項演算子の優先度（高いほど結合力が強い）。
fn binary_prec(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::LogOr => 1,
        BinaryOp::LogAnd => 2,
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Le
        | BinaryOp::Gt
        | BinaryOp::Ge => 3,
        BinaryOp::BitOr | BinaryOp::BitXor => 4,
        BinaryOp::BitAnd => 5,
        BinaryOp::Sll | BinaryOp::Sra | BinaryOp::Srl => 6,
        BinaryOp::Concat => 7,
        BinaryOp::Add | BinaryOp::Sub => 8,
        BinaryOp::Mul => 9,
        BinaryOp::SignExt => 10,
    }
}

fn binary_op_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::LogOr => "||",
        BinaryOp::LogAnd => "&&",
        BinaryOp::Sll => "<<",
        BinaryOp::Srl => ">>>",
        BinaryOp::Sra => ">>",
        BinaryOp::Concat => "++",
        BinaryOp::SignExt => "#",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source() {
        assert_eq!(format(""), Some(String::new()));
    }

    #[test]
    fn empty_module() {
        assert_eq!(format("module M {}"), Some("module M {}\n".to_string()));
    }

    #[test]
    fn empty_trait() {
        assert_eq!(format("trait T {}"), Some("trait T {}\n".to_string()));
    }

    #[test]
    fn parse_error_returns_none() {
        // 閉じブレース欠落
        assert_eq!(format("module M {"), None);
    }

    #[test]
    fn lex_error_returns_none() {
        // 不正なトークン
        assert_eq!(format("@@@"), None);
    }

    #[test]
    fn module_with_single_val() {
        let src = "module M { val x = 1 }";
        let expected = "module M {\n  val x = 1\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn module_with_reg_decl() {
        let src = "module M { reg count: Bit(8) }";
        let expected = "module M {\n  reg count: Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn module_with_io_decls() {
        let src = "module M {\n  input a: Bit(8)\n  output b: Bit(8)\n}";
        let expected = "module M {\n  input a: Bit(8)\n  output b: Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn val_with_variable_rhs() {
        let src = "module M { val y = x }";
        let expected = "module M {\n  val y = x\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn binary_add_has_spaces_around() {
        let src = "module M { val z = a + b }";
        let expected = "module M {\n  val z = a + b\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn unary_not_no_space() {
        let src = "module M { val z = !x }";
        let expected = "module M {\n  val z = !x\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn module_with_extends() {
        let src = "module M extends S {}";
        let expected = "module M extends S {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn module_with_extends_and_with() {
        let src = "module M extends S with T1 with T2 {}";
        let expected = "module M extends S with T1 with T2 {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn function_call_with_args() {
        let src = "module M { val z = f(a, b) }";
        let expected = "module M {\n  val z = f(a, b)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn always_block_with_single_stmt() {
        let src = "module M extends S { always { x } }";
        let expected = "module M extends S {\n  always {\n    x\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reg_assign_with_coloneq() {
        let src = "module M extends S { always { count := count + 1 } }";
        let expected = "module M extends S {\n  always {\n    count := count + 1\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn if_without_else() {
        let src = "module M extends S { always { if (a == b) x } }";
        let expected = "module M extends S {\n  always {\n    if (a == b) x\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn if_with_else() {
        let src = "module M extends S { always { if (a) x else y } }";
        let expected = "module M extends S {\n  always {\n    if (a) x else y\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn string_literal_preserved() {
        let src = r#"module M extends S { always { _display("hi") } }"#;
        let expected = "module M extends S {\n  always {\n    _display(\"hi\")\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn boolean_literal() {
        let src = "module M { val x = true }";
        let expected = "module M {\n  val x = true\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn tuple_expr_and_field_access() {
        let src = "module M { val z = (a.x, b.y) }";
        let expected = "module M {\n  val z = (a.x, b.y)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn bit_literal_as_hex() {
        // BitLit はビット幅情報を失っているため一律 0x<hex> で出力
        let src = "module M { val z = 0x10 }";
        let expected = "module M {\n  val z = 0x10\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn def_with_typed_params() {
        let src = "module M { def f(a: Bit(8), b: Bit(8)): Bit(8) = a + b }";
        let expected = "module M {\n  def f(a: Bit(8), b: Bit(8)): Bit(8) = a + b\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn private_def_with_receiver() {
        let src = "module M { private def cpu.read(): Unit = x }";
        let expected = "module M {\n  private def cpu.read(): Unit = x\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn match_expr() {
        let src = "module M extends S { always { f match { case 0 => a case 1 => b case _ => c } } }";
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    f match {\n",
            "      case 0 => a\n",
            "      case 1 => b\n",
            "      case _ => c\n",
            "    }\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn new_instance_field() {
        let src = "module M extends S { val s = new Sub }";
        let expected = "module M extends S {\n  val s = new Sub\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn val_decl_inside_block() {
        let src = "module M extends S { always { val x = a + 1 } }";
        let expected = "module M extends S {\n  always {\n    val x = a + 1\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn mem_decl_field() {
        let src = "module M extends S { mem [Bit(70)] pat(16) }";
        let expected = "module M extends S {\n  mem [Bit(70)] pat(16)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn add32_sample_reparses() {
        let src = include_str!("../../../fsl-sample/alu32-main/add32.fsl");
        let formatted = format(src).expect("add32.fsl should format");
        let (result, lex_errs) = fsl_parser::parse(&formatted);
        assert!(lex_errs.is_empty(), "lex errors in formatted output: {:?}", lex_errs);
        assert!(
            result.errors.is_empty(),
            "parse errors in formatted output: {:?}\n--- formatted ---\n{}",
            result.errors,
            formatted
        );
    }

    #[test]
    fn blank_line_between_top_level_items() {
        let src = "module A {} module B {}";
        let expected = "module A {}\n\nmodule B {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn def_with_seq_body() {
        let src = "module M { def f(): Unit seq { x } }";
        let expected = "module M {\n  def f(): Unit seq {\n    x\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn def_with_par_body() {
        let src = "module M { def f(): Unit par { x } }";
        let expected = "module M {\n  def f(): Unit par {\n    x\n  }\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn parens_preserved_for_lower_prec_left() {
        // (a + b) * c は Mul(Add(a,b), c) なので括弧必須
        let src = "module M { val z = (a + b) * c }";
        let expected = "module M {\n  val z = (a + b) * c\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn parens_preserved_for_same_prec_right() {
        // a - (b - c) は Sub(a, Sub(b,c))、左結合のため右側は括弧必須
        let src = "module M { val z = a - (b - c) }";
        let expected = "module M {\n  val z = a - (b - c)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn no_parens_for_same_prec_left() {
        // a - b - c は Sub(Sub(a,b), c)、左側は同優先度でも括弧不要
        let src = "module M { val z = a - b - c }";
        let expected = "module M {\n  val z = a - b - c\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn types_array_list_tuple() {
        let src = "module M { reg a: Array[Bit(8)] reg b: List[Int] reg c: (Int, Boolean) }";
        let expected = concat!(
            "module M {\n",
            "  reg a: Array[Bit(8)]\n",
            "  reg b: List[Int]\n",
            "  reg c: (Int, Boolean)\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn test_alu32_sample_reparses() {
        let src = include_str!("../../../fsl-sample/alu32-main/test_alu32.fsl");
        let formatted = format(src).expect("test_alu32.fsl should format");
        let (result, lex_errs) = fsl_parser::parse(&formatted);
        assert!(lex_errs.is_empty(), "lex errors in formatted output: {:?}", lex_errs);
        assert!(
            result.errors.is_empty(),
            "parse errors in formatted output: {:?}\n--- formatted ---\n{}",
            result.errors,
            formatted
        );
    }

    #[test]
    fn line_comment_before_module_preserved() {
        let src = "// header comment\nmodule M {}";
        let expected = "// header comment\nmodule M {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn block_comment_at_top_preserved() {
        let src = "/* header */\nmodule M {}";
        let expected = "/* header */\nmodule M {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn comment_inside_module_body_preserved() {
        let src = "module M {\n  // count is the counter\n  reg count: Bit(8)\n}";
        let expected = "module M {\n  // count is the counter\n  reg count: Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_comment_preserved() {
        let src = "module M {}\n// trailing";
        let expected = "module M {}\n// trailing\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn inline_expr_comment_moves_above() {
        let src = "module M { val z = a /* tag */ + b }";
        let expected = "module M {\n  /* tag */\n  val z = a + b\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn comment_before_closing_brace_stays_in_block() {
        // `/* foo */ }` — `/* foo */` は always ブロックの内側に残るべき
        let src = "module M extends S {\n  always {\n    x\n    /* foo */\n  }\n}";
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    x\n",
            "    /* foo */\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn line_trailing_comment_after_inner_brace_stays_in_outer() {
        // `} // tag` — `// tag` は親 (module) ブロック内、`}` の直前にとどまる
        let src = "module M extends S {\n  always { x } // tag\n}";
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    x\n",
            "  }\n",
            "  // tag\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn empty_module_body_with_comment_expands() {
        // 空モジュール本体にコメントがある場合は単一行 `{}` ではなく多行展開
        let src = "module M { /* note */ }";
        let expected = "module M {\n  /* note */\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_moves_above_field() {
        // `input a: Bit(8) // hoge` の `// hoge` は AST span 外だが同一行末。
        // 入れ子ブロックを持たない文の同一行末コメントは直上に移動する。
        let src = "module M {\n  input a: Bit(8) // hoge\n  input b: Bit(8)\n}";
        let expected = concat!(
            "module M {\n",
            "  // hoge\n",
            "  input a: Bit(8)\n",
            "  input b: Bit(8)\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_moves_above_stmt_in_block() {
        // ブロック内文 `cout = m; // bar` も同様に直上に移動
        let src = "module M extends S {\n  always {\n    cout = m // bar\n    sum = n\n  }\n}";
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    // bar\n",
            "    cout = m\n",
            "    sum = n\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }
}
