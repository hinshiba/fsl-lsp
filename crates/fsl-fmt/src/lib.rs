//! FSL ソースコードのフォーマッタ。
//!
//! 公開 API は [`format`] のみ。パース／レキサーエラー時は何も返さない。

use fsl_parser::{
    BinaryOp, CompositeDef, CompositeField, Expr, Expr_, Field, FnDef, FslType, FslType_,
    InputDecl, Item, MemDecl, ModuleDef, NewInstance, OutputDecl, OutputFnDecl, Param, Pattern,
    RegDecl, Spanned, StageDef, StageItem, StateDef, TraitDef, UnaryOp, ValDecl, ValLhs,
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
    /// 各コメントが既に出力済みか。並び替えで出力順とソース順が乖離するため、
    /// 単一カーソルではなくフラグ配列で管理する。
    emitted: Vec<bool>,
}

impl<'a> Writer<'a> {
    fn new(src: &'a str) -> Self {
        // ソースを直接 lex してコメントだけ収集する（パーサは trivia を捨てているため）
        let lex_result = fsl_lexer::lex(src);
        let comments: Vec<(usize, String)> = lex_result
            .oks
            .into_iter()
            .filter_map(|t| match t.tok {
                fsl_lexer::Token::LineComment(s) | fsl_lexer::Token::BlockComment(s) => {
                    Some((t.span.start, s))
                }
                _ => None,
            })
            .collect();
        let emitted = vec![false; comments.len()];
        Writer {
            src,
            out: String::new(),
            comments,
            emitted,
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
    /// なければ span.end まで（同一物理行末のトレイリングは別経路で inline 出力）。
    fn inline_limit_for_expr(&self, e: &Expr, block_end: usize) -> usize {
        match first_nested_block_start_in_expr(e) {
            Some(p) => p,
            None => e.span.end.min(block_end),
        }
    }

    fn inline_limit_for_field(&self, f: &Field, span_end: usize, body_end: usize) -> usize {
        match first_nested_block_start_in_field(f) {
            Some(p) => p,
            None => span_end.min(body_end),
        }
    }

    /// `span_end` 以降〜同じ物理行末 (`\n` 直前) にある未出力コメントを 1 つ取り出す。
    /// `upper` は呼び出し側ブロックの上限位置で、これを越えて取り込まない（親 `}` を越えないため）。
    /// 入れ子ブロックを持たない文のトレイリングコメントを「インライン維持」する用途。
    fn take_trailing_inline_comment(&mut self, span_end: usize, upper: usize) -> Option<String> {
        let line_end = self.line_end_after(span_end).min(upper);
        for i in 0..self.comments.len() {
            if self.emitted[i] {
                continue;
            }
            let cpos = self.comments[i].0;
            if cpos >= line_end {
                break;
            }
            if cpos >= span_end {
                self.emitted[i] = true;
                return Some(self.comments[i].1.clone());
            }
        }
        None
    }

    /// ソース範囲 `[lo, hi)` 中で「空行」と数えるべき行数。
    /// コメントは区切りとみなし、各区間で改行数 - 1 を独立に数えて加算する。
    /// 例: `\n\n` → 1 空行、`\n\n\n` → 2 空行、`\n` → 0 空行。
    fn count_blank_lines_between(&self, lo: usize, hi: usize) -> usize {
        if lo >= hi {
            return 0;
        }
        let slice = &self.src[lo..hi];
        // コメント位置を取り出してセクション分割する。コメント自身は「コンテンツ」とみなす。
        let mut cuts: Vec<(usize, usize)> = Vec::new();
        for (cpos, ctext) in &self.comments {
            if *cpos >= lo && *cpos < hi {
                let start = cpos - lo;
                let end = start + ctext.len();
                cuts.push((start, end.min(slice.len())));
            }
        }
        let mut segments: Vec<&str> = Vec::new();
        let mut cursor = 0;
        for (s, e) in &cuts {
            if cursor < *s {
                segments.push(&slice[cursor..*s]);
            }
            cursor = *e;
        }
        if cursor < slice.len() {
            segments.push(&slice[cursor..]);
        }
        if segments.is_empty() {
            segments.push(slice);
        }
        let mut total = 0;
        for seg in segments {
            let n = seg.matches('\n').count();
            total += n.saturating_sub(1);
        }
        total
    }

    /// `out` の現在書き込み行頭からの文字数 (0-indexed col)。
    fn current_line_col(&self) -> usize {
        match self.out.rfind('\n') {
            Some(p) => self.out.len() - p - 1,
            None => self.out.len(),
        }
    }

    /// インライン末尾コメントを emit する。
    /// 最低 1 スペース + `/` が 0-indexed 偶数列に揃うようパディング。
    fn emit_inline_trailing(&mut self, text: &str) {
        let col = self.current_line_col();
        // 最低 1 スペース後の `/` 候補列
        let min_target = col + 1;
        let target = if min_target % 2 == 0 { min_target } else { min_target + 1 };
        for _ in col..target {
            self.out.push(' ');
        }
        self.out.push_str(text);
    }

    /// 整形を終了し、末尾に残ったコメントを行頭に流し込んで返す。
    fn finish(mut self) -> String {
        self.flush_remaining_comments(0);
        self.out
    }

    /// `pos` より前にあって未出力のコメントを行頭に書き出す（ソース順）。
    /// 各コメントは独立行とし、ブロックコメント内の既存改行は維持。
    fn flush_comments_before(&mut self, pos: usize, depth: usize) {
        for i in 0..self.comments.len() {
            if self.emitted[i] {
                continue;
            }
            if self.comments[i].0 >= pos {
                break;
            }
            let text = self.comments[i].1.clone();
            self.write_indent(depth);
            self.out.push_str(&text);
            self.out.push('\n');
            self.emitted[i] = true;
        }
    }

    /// 残った未出力コメントを書き出す（ファイル末尾）。
    fn flush_remaining_comments(&mut self, depth: usize) {
        for i in 0..self.comments.len() {
            if self.emitted[i] {
                continue;
            }
            let text = self.comments[i].1.clone();
            self.write_indent(depth);
            self.out.push_str(&text);
            self.out.push('\n');
            self.emitted[i] = true;
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
        let mut prev_end: Option<usize> = None;
        for item in items.iter() {
            // トップレベルでもアイテム前のコメントを排出（中身は item 自身が depth+1 で処理）
            self.flush_comments_before(item.span.start, 0);
            if let Some(prev) = prev_end {
                let blanks = self.count_blank_lines_between(prev, item.span.start).min(1);
                for _ in 0..blanks {
                    self.out.push('\n');
                }
            }
            self.write_item(&item.inner, item.span.end, 0);
            prev_end = Some(item.span.end);
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
    /// アイテムは 4 グループ (I/O → OutputFn → Reg/Mem/Val/Composite → Fn/Stage/Always/Initial) に
    /// 安定ソート、グループ境界に空行 2 行。
    fn write_fields(&mut self, items: &[Spanned<Field>], body_end: usize, depth: usize) {
        if items.is_empty() && !self.has_comments_before(body_end) {
            self.out.push_str(" {}");
            return;
        }
        self.out.push_str(" {\n");

        // 並び替えに備えて各 field にコメントを事前 attach
        let attached = self.attach_comments_to_fields(items, body_end);

        // グループ番号 + 元順序で安定ソート
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&i| (field_group(&items[i].inner), i));

        let mut prev: Option<(u8, usize)> = None;
        for &i in &order {
            let cur_g = field_group(&items[i].inner);
            if let Some((pg, p_idx)) = prev {
                // 同一グループ: ソース空行を max 1 で保存。
                // グループ境界 (ソース順維持): ソース空行を max 2 で保存。
                // グループ境界 (並び替え発生): 強制 2 空行（視覚分離のため）。
                let prev_end = items[p_idx].span.end;
                let next_start = items[i].span.start;
                let blanks = if pg != cur_g {
                    if prev_end > next_start {
                        2
                    } else {
                        self.count_blank_lines_between(prev_end, next_start).min(2)
                    }
                } else {
                    self.count_blank_lines_between(prev_end, next_start).min(1)
                };
                for _ in 0..blanks {
                    self.out.push('\n');
                }
            }
            // attached leading コメントを emit
            for &cidx in &attached[i].0 {
                if !self.emitted[cidx] {
                    let text = self.comments[cidx].1.clone();
                    self.write_indent(depth + 1);
                    self.out.push_str(&text);
                    self.out.push('\n');
                    self.emitted[cidx] = true;
                }
            }
            // field 本体
            self.write_indent(depth + 1);
            self.write_field(&items[i].inner, depth + 1);
            // attached trailing inline コメントを emit
            if let Some(tidx) = attached[i].1 {
                if !self.emitted[tidx] {
                    let text = self.comments[tidx].1.clone();
                    self.emit_inline_trailing(&text);
                    self.emitted[tidx] = true;
                }
            }
            self.out.push('\n');
            prev = Some((cur_g, i));
        }

        // ブロック内末尾の未出力コメントを `}` 直前に排出
        self.flush_comments_before(body_end, depth + 1);
        self.write_indent(depth);
        self.out.push('}');
    }

    /// 各 field に対し、リーディングコメント (Vec<index>) とトレイリングインラインコメント
    /// (Option<index>) を attach。並び替え後でも元の所属を保てるように事前計算する。
    fn attach_comments_to_fields(
        &self,
        items: &[Spanned<Field>],
        body_end: usize,
    ) -> Vec<(Vec<usize>, Option<usize>)> {
        let n = items.len();
        let mut result: Vec<(Vec<usize>, Option<usize>)> =
            (0..n).map(|_| (Vec::new(), None)).collect();
        let mut next = 0;

        for i in 0..n {
            // limit = inline_limit_for_field 相当: 入れ子ブロック開始位置 or span.end
            let limit = self.inline_limit_for_field(&items[i].inner, items[i].span.end, body_end);
            let span_end = items[i].span.end;
            let line_end = self.line_end_after(span_end).min(body_end);

            // 既存 emitted を尊重しつつ、未 attach のコメントを leading に追加
            while next < self.comments.len() {
                if self.emitted[next] {
                    next += 1;
                    continue;
                }
                if self.comments[next].0 < limit {
                    result[i].0.push(next);
                    next += 1;
                } else {
                    break;
                }
            }

            // trailing inline (入れ子ブロックを持たない field のみ)
            let has_nested = first_nested_block_start_in_field(&items[i].inner).is_some();
            if !has_nested && next < self.comments.len() && !self.emitted[next] {
                let cpos = self.comments[next].0;
                if cpos >= span_end && cpos < line_end {
                    result[i].1 = Some(next);
                    next += 1;
                }
            }
        }

        result
    }

    /// `pos` より前に未出力コメントが残っているか。
    fn has_comments_before(&self, pos: usize) -> bool {
        self.comments
            .iter()
            .enumerate()
            .any(|(i, c)| !self.emitted[i] && c.0 < pos)
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
            Field::OutputFn(o) => self.write_output_fn(o),
            Field::Stage(s) => self.write_stage(s, depth),
            Field::Composite(c) => self.write_composite(c),
            Field::Error => {}
        }
    }

    /// `output def name(params): T` または `output def name()` の整形。
    fn write_output_fn(&mut self, o: &OutputFnDecl) {
        self.out.push_str("output def ");
        self.out.push_str(&o.name.inner);
        self.out.push('(');
        self.write_params(&o.params);
        self.out.push(')');
        if let Some(ret) = &o.ret {
            self.out.push_str(": ");
            self.write_type(ret);
        }
    }

    /// `stage name(params) { body }` の整形。
    fn write_stage(&mut self, s: &StageDef, depth: usize) {
        self.out.push_str("stage ");
        self.out.push_str(&s.name.inner);
        self.out.push('(');
        self.write_params(&s.params);
        self.out.push(')');
        // 空ステージは `{}` 単一行
        if s.body.is_empty() {
            self.out.push_str(" {}");
            return;
        }
        self.out.push_str(" {\n");
        for item in &s.body {
            self.write_indent(depth + 1);
            self.write_stage_item(&item.inner, depth + 1);
            self.out.push('\n');
        }
        self.write_indent(depth);
        self.out.push('}');
    }

    fn write_stage_item(&mut self, item: &StageItem, depth: usize) {
        match item {
            StageItem::Reg(r) => self.write_reg(r, depth),
            StageItem::State(s) => self.write_state(s, depth),
            StageItem::Expr(e) => self.write_expr_owned(e, depth),
        }
    }

    fn write_state(&mut self, s: &StateDef, depth: usize) {
        self.out.push_str("state ");
        self.out.push_str(&s.name.inner);
        self.out.push_str(" = ");
        self.write_expr(&s.body, depth);
    }

    /// `Expr` を所有値経由で書き出す（`StageItem::Expr(Expr)` の Expr を直接渡せるように）。
    fn write_expr_owned(&mut self, e: &Expr, depth: usize) {
        self.write_expr(e, depth);
    }

    /// `type name(field: T, ...)` の整形。
    fn write_composite(&mut self, c: &CompositeDef) {
        self.out.push_str("type ");
        self.out.push_str(&c.name.inner);
        self.out.push('(');
        for (i, f) in c.fields.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.write_composite_field(f);
        }
        self.out.push(')');
    }

    fn write_composite_field(&mut self, f: &CompositeField) {
        self.out.push_str(&f.name.inner);
        self.out.push_str(": ");
        self.write_type(&f.ty);
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
                // unary は全 binary より結合度が強いので binary 子は要 ()
                // また unary 連続 (`~|x`) は FSL コンパイラ非対応なので unary 子も要 ()
                let needs_parens = matches!(
                    &rhs.inner,
                    Expr_::Binary(_, _, _) | Expr_::Unary(_, _)
                );
                if needs_parens {
                    self.out.push('(');
                    self.write_expr(rhs, depth);
                    self.out.push(')');
                } else {
                    self.write_expr(rhs, depth);
                }
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
            // 式内コメントは直上に持ち上げ、同一物理行末のトレイリングはインライン維持。
            let limit = self.inline_limit_for_expr(s, block_end);
            self.flush_comments_before(limit, depth + 1);
            self.write_indent(depth + 1);
            self.write_expr(s, depth + 1);
            if first_nested_block_start_in_expr(s).is_none() {
                if let Some(text) = self.take_trailing_inline_comment(s.span.end, block_end) {
                    self.emit_inline_trailing(&text);
                }
            }
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

/// フィールドの並び替えグループ番号。小さいほど先頭。
/// G0: I/O (Input, Output)
/// G1: OutputFn (出力関数)
/// G2: 宣言 (Reg, Mem, Val, NewInstance, Composite)
/// G3: 実装 (Fn, Stage, Always, Initial)
/// G4: その他 (Error など)
fn field_group(f: &Field) -> u8 {
    match f {
        Field::Input(_) | Field::Output(_) => 0,
        Field::OutputFn(_) => 1,
        Field::Reg(_)
        | Field::Mem(_)
        | Field::Val(_)
        | Field::NewInstance(_)
        | Field::Composite(_) => 2,
        Field::Fn(_) | Field::Stage(_) | Field::Always(_) | Field::Initial(_) => 3,
        Field::Error => 4,
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
    fn no_blank_line_between_adjacent_top_level_items() {
        // ソースに空行がない場合は出力にも空行を入れない（新仕様）。
        let src = "module A {} module B {}";
        let expected = "module A {}\nmodule B {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn one_blank_line_preserved_between_top_level_items() {
        let src = "module A {}\n\nmodule B {}";
        let expected = "module A {}\n\nmodule B {}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn output_fn_with_return_type() {
        let src = "module M { output def f(a, b): Bit(8) }";
        let expected = "module M {\n  output def f(a, b): Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn output_fn_without_return_type() {
        let src = "module M { output def halt() }";
        let expected = "module M {\n  output def halt()\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn stage_with_body() {
        let src = "module M { stage s(a: Bit(8)) { _display(\"hi\", a) } }";
        let expected = concat!(
            "module M {\n",
            "  stage s(a: Bit(8)) {\n",
            "    _display(\"hi\", a)\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn composite_type_decl() {
        let src = "module M { type pair(x: Bit(8), y: Bit(8)) }";
        let expected = "module M {\n  type pair(x: Bit(8), y: Bit(8))\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_full_four_groups() {
        // 4 グループの全種が逆順に並んでいるケース
        let src = concat!(
            "module M {\n",
            "  def f(): Unit = x\n",
            "  reg r: Bit(8)\n",
            "  output def g(): Bit(8)\n",
            "  input a: Bit(8)\n",
            "}",
        );
        let expected = concat!(
            "module M {\n",
            "  input a: Bit(8)\n",
            "\n\n",
            "  output def g(): Bit(8)\n",
            "\n\n",
            "  reg r: Bit(8)\n",
            "\n\n",
            "  def f(): Unit = x\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_stable_within_group_inputs() {
        // input/output は同じ G0、入れ替わらない
        let src = "module M {\n  output a: Bit(8)\n  input b: Bit(8)\n}";
        let expected = "module M {\n  output a: Bit(8)\n  input b: Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_stable_within_group_def_always() {
        // def と always は同じ G3、入れ替わらない
        let src = concat!(
            "module M extends S {\n",
            "  always { x }\n",
            "  def f(): Unit = y\n",
            "}",
        );
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    x\n",
            "  }\n",
            "  def f(): Unit = y\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_single_group_no_boundary_blanks() {
        // 単一グループのみのモジュールにはグループ境界の空行 2 が登場しない
        let src = "module M {\n  input a: Bit(8)\n  output b: Bit(8)\n}";
        let expected = "module M {\n  input a: Bit(8)\n  output b: Bit(8)\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_leading_comment_follows_item() {
        // input の直上コメントは input に追従して並び替え後も同じ位置に来る
        let src = concat!(
            "module M {\n",
            "  def f(): Unit = x\n",
            "  // hello\n",
            "  input a: Bit(8)\n",
            "}",
        );
        let expected = concat!(
            "module M {\n",
            "  // hello\n",
            "  input a: Bit(8)\n",
            "\n\n",
            "  def f(): Unit = x\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_trailing_inline_comment_follows_item() {
        // def の末尾インラインコメントが def と一緒に動く
        // 「  def f(): Unit = x」19 文字、`/` 候補は col 20 (偶数) なので 1 スペース
        let src = concat!(
            "module M {\n",
            "  def f(): Unit = x // bye\n",
            "  input a: Bit(8)\n",
            "}",
        );
        let expected = concat!(
            "module M {\n",
            "  input a: Bit(8)\n",
            "\n\n",
            "  def f(): Unit = x // bye\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn reorder_def_before_input_puts_input_first() {
        // ソース順: def → input。G1 (input) を G4 (def) より先に並べる。
        let src = "module M {\n  def f(): Unit = x\n  input a: Bit(8)\n}";
        let expected = concat!(
            "module M {\n",
            "  input a: Bit(8)\n",
            "\n\n",
            "  def f(): Unit = x\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn many_blank_lines_clamped_to_one_top_level() {
        let src = "module A {}\n\n\n\nmodule B {}";
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
    fn unary_keeps_parens_around_binary_child() {
        // `~(a ^ b)` の () は意味上必要（unary は binary より結合度が強いため）。
        let src = "module M { val z = ~(a ^ b) & c }";
        let expected = "module M {\n  val z = ~(a ^ b) & c\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn unary_keeps_parens_around_unary_child() {
        // FSL コンパイラは `~|x` のような unary 連続をサポートしないため
        // `~(|x)` の () を維持する。
        let src = "module M { val z = ~(|x) }";
        let expected = "module M {\n  val z = ~(|x)\n}\n";
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
    fn alu32_sample_reparses_and_idempotent() {
        // 並び替え順 (I/O → reg/mem/val → def) になっており、空行も正しい。
        // 1 度整形した結果を再整形しても変わらない。
        let src = include_str!("../../../fsl-sample/alu32-main/alu32.fsl");
        let formatted = format(src).expect("alu32.fsl should format");
        let (result, lex_errs) = fsl_parser::parse(&formatted);
        assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
        assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
        // idempotent: 2 度目の整形でも変化しない
        let formatted2 = format(&formatted).expect("second format");
        assert_eq!(formatted, formatted2, "alu32.fsl is not idempotent under format");
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
    fn trailing_line_comment_stays_inline_val() {
        // `val x = 1 // tag` は同一行維持。
        // インデント込みで「  val x = 1」は 11 文字、`/` は col 12 (0-indexed, 偶数) で
        // すでに最低 1 スペース条件を満たすので 1 スペース。
        let src = "module M {\n  val x = 1 // tag\n}";
        let expected = "module M {\n  val x = 1 // tag\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_block_comment_stays_inline_val() {
        // `val x = 1 /* tag */` も同一行維持。
        let src = "module M {\n  val x = 1 /* tag */\n}";
        let expected = "module M {\n  val x = 1 /* tag */\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_stays_inline_input() {
        // `input a: Bit(8) // hoge` も同一行維持（旧 "上に移動" 挙動の差し替え）。
        // 「  input a: Bit(8)」は 17 文字、`/` を col 18 (偶数) に置くため 1 スペース。
        let src = "module M {\n  input a: Bit(8) // hoge\n  input b: Bit(8)\n}";
        let expected = concat!(
            "module M {\n",
            "  input a: Bit(8) // hoge\n",
            "  input b: Bit(8)\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_stays_inline_reg() {
        // `reg count: Bit(8) // c`
        // 「  reg count: Bit(8)」は 19 文字、`/` は col 20 (偶数) で 1 スペース。
        let src = "module M {\n  reg count: Bit(8) // c\n}";
        let expected = "module M {\n  reg count: Bit(8) // c\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn one_blank_at_group_boundary_preserved() {
        // グループ境界でも 1 空行を維持（強制 2 行に増やさない）。
        let src = "module M {\n  input a: Bit(8)\n\n  val x = 1\n}";
        let expected = "module M {\n  input a: Bit(8)\n\n  val x = 1\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn two_blanks_at_group_boundary_preserved() {
        // ソースに 2 空行があれば 2 空行を維持。
        let src = "module M {\n  input a: Bit(8)\n\n\n  val x = 1\n}";
        let expected = "module M {\n  input a: Bit(8)\n\n\n  val x = 1\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn three_blanks_at_group_boundary_clamped_to_two() {
        let src = "module M {\n  input a: Bit(8)\n\n\n\n  val x = 1\n}";
        let expected = "module M {\n  input a: Bit(8)\n\n\n  val x = 1\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn one_blank_line_preserved_between_fields() {
        let src = "module M {\n  val x = 1\n\n  val y = 2\n}";
        let expected = "module M {\n  val x = 1\n\n  val y = 2\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn no_blank_line_between_adjacent_fields() {
        let src = "module M {\n  val x = 1\n  val y = 2\n}";
        let expected = "module M {\n  val x = 1\n  val y = 2\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn two_blank_lines_clamped_to_one_within_group() {
        let src = "module M {\n  val x = 1\n\n\n  val y = 2\n}";
        let expected = "module M {\n  val x = 1\n\n  val y = 2\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn many_blank_lines_clamped_to_one_within_group() {
        let src = "module M {\n  val x = 1\n\n\n\n\n  val y = 2\n}";
        let expected = "module M {\n  val x = 1\n\n  val y = 2\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_alignment_two_spaces() {
        // インデント込みで「  val x = 12」12 文字、`/` を col 14 (偶数) に置くため 2 スペース。
        let src = "module M {\n  val x = 12 // tag\n}";
        let expected = "module M {\n  val x = 12  // tag\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_stays_inline_mem() {
        // `mem [Bit(70)] pat(16) // m`
        // 「  mem [Bit(70)] pat(16)」23 文字、`/` は col 24 (偶数) で 1 スペース。
        let src = "module M extends S {\n  mem [Bit(70)] pat(16) // m\n}";
        let expected = "module M extends S {\n  mem [Bit(70)] pat(16) // m\n}\n";
        assert_eq!(format(src), Some(expected.to_string()));
    }

    #[test]
    fn trailing_line_comment_stays_inline_stmt_in_block() {
        // ブロック内文 `cout = m // bar` もインライン維持。
        // 「    cout = m」12 文字、`/` を col 14 (偶数) に置くため 2 スペース。
        let src = "module M extends S {\n  always {\n    cout = m // bar\n    sum = n\n  }\n}";
        let expected = concat!(
            "module M extends S {\n",
            "  always {\n",
            "    cout = m  // bar\n",
            "    sum = n\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(format(src), Some(expected.to_string()));
    }
}
