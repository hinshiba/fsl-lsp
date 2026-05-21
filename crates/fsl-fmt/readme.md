# fsl-fmt 実装メモ

## 指示

`crates/fsl-fmt` を新規追加し、独自 HDL `FSL` のフォーマッタを実装。

- 開発手法: t_wada 流 TDD（テストリスト → Red → Green → Refactor）
- 公開 API: `pub fn format(src: &str) -> Option<String>`（失敗時は `None`、部分整形なし）
- スタイル: インデント 2 / 行幅 100 / K&R / 演算子前後空白
- コメント保存（パーサ変更は許容されていたが結果的に不要）
- 冪等性は不要、パースエラー時は何もしなくてよい
- FSL パーサ制約: 関数呼び出しの `(` 直後・`)` 直前で改行不可（改行はカンマ直後のみ）

進め方: テストリスト (`tests.md`) を作成 → User 承認 → 1 サイクル 1 テストで反復。

## コメント保存

パーサは依然 `strip_trivia` でコメントを捨てる。代わりに **フォーマッタ側で再 lex** してコメント位置を拾い、AST スパンと突き合わせて配置する。

```rust
struct Writer<'a> {
    src: &'a str,
    out: String,
    comments: Vec<(usize, String)>, // (start_pos, text) 位置順
    next_comment: usize,            // 単調増加カーソル
}
```

`flush_comments_before(pos, depth)` で「`pos` より前の未出力コメントを `depth` 字下げで吐き出す」。これを 4 箇所で呼ぶ:

1. トップレベル各 item の `span.start` 直前
2. ブロック反復: 各文の **inline_limit** 直前
3. ブロック反復後: `block_end` 直前 ← ブロック境界越え防止
4. `finish()`: 残り全部

### inline_limit

```rust
fn inline_limit_for_expr(&self, e: &Expr, block_end: usize) -> usize {
    match first_nested_block_start_in_expr(e) {
        Some(p) => p,                                              // 入れ子の `{` まで
        None    => self.line_end_after(e.span.end).min(block_end), // 物理行末まで
    }
}
```

- 入れ子ブロックあり → 上限は `{` 位置（内側コメントは内側の `write_block` に任せる）
- 入れ子ブロックなし → 物理行末まで延長（同一行末トレイリング `// hoge` を取り込む）
- `min(block_end)` で親 `}` 越えを clamp

`first_nested_block_start_in_expr` は AST を再帰下降して最初の `Block`/`Seq`/`Par`/`Match` の `span.start` を返す純粋関数。

これで以下が同時に成立:
- `val z = a /* tag */ + b` の `/* tag */` → val の直上
- `input a: Bit(8) // hoge` の `// hoge` → input a の直上
- `always { x /* foo */ }` の `/* foo */` → always 内 `}` 直前（境界越えなし）
- `} // add` の `// add` → 親モジュール内 `}` 直前
