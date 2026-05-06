# CLAUDE.md

このファイルはClaudeが本リポジトリで作業する際に参照すべき情報をまとめる．

## chumsky パーサライブラリ

本プロジェクトの `fsl-parser` クレートはパーサコンビネータライブラリ `chumsky 0.12.0` (feature: `pratt`) を用いる．

### ソースコード検索パス

chumskyのソースは以下のローカルキャッシュに展開されている．APIや内部実装を確認したい場合はここを直接読む．

```
C:\Users\haruh\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\chumsky-0.12.0\
```

主要モジュール．

- `src/lib.rs`：`Parser` トレイト本体．`spanned`, `boxed`, `map`, `then`, `or_not` などの全コンビネータ
- `src/recursive.rs`：再帰パーサ (`Recursive`, `Indirect`, `Direct`, `recursive()`)
- `src/primitive.rs`：原始パーサ (`just`, `Todo`, `select`, `select_ref`, `choice`, `none_of`, `one_of` 等)
- `src/combinator.rs`：コンビネータ実装 (`Map`, `Then`, `IgnoreThen`, `Spanned`, `Repeated` 等)
- `src/pratt.rs`：Pratt 演算子パーサ (`infix`, `prefix`, `postfix`, `left`, `right`)
- `src/input.rs`：入力アダプタ (`Input`, `ValueInput`, `MappedInput`)
- `src/span.rs`：`Span`, `SimpleSpan`, `Spanned`
- `src/error.rs`：エラー型 (`Rich`, `Simple`)

ローカルにcloneしたchumskyリポジトリ ( `chumsky/` ) もあるが，これは公式リポジトリではなく `examples/` と `guide/` のみ．APIの一次情報はcargo registry側を参照すること．

### 再帰パーサの使い分け

chumskyには2つの再帰機構がある．

1. `chumsky::recursive::recursive(|p| ...)`：自己再帰．関数内で完結する1つのパーサが自身を参照する場合．戻り値は `Recursive<Direct<...>>`．

2. `Recursive::declare()` + `define()`：相互再帰．関数をまたいで複数のパーサが互いを参照する場合．先に未定義ハンドルを作り，各構築関数に `.clone()` で配り，最後に `define()` で実体を結ぶ．戻り値は `Recursive<Indirect<...>>`．

`Recursive` は内部で `Rc` を持つため `.clone()` は安価．

### 既知の制約

- `recursive()` および `Recursive::define()` は parser に `Clone` 境界を要求する．`impl Parser` で返す関数は `+ Clone` を付ける必要がある．付けられない場合は呼出側で `.boxed()` で包んで `Boxed<...>` (Rc<dyn Parser> ; 常に Clone) にする．
- `Indirect<'src, 'b, ...>` の `'b` は内部パーサの寿命．所有パーサ (Rc 経由で持つ) では `'b = 'src` (本プロジェクトでは `'tok = 'tok`) で問題ない．

## プロジェクト構成

`crates/fsl-parser/src/parsers/` 以下にモジュール分割されたパーサ群がある．元の `parsers.rs::parser2` (旧実装) を分割中．

### 相互再帰の配線

`parsers.rs` 上部で型エイリアスと配線ヘルパーを定義．

- `RecBlock<'tok, I>` / `RecExpr<'tok, I>`：相互再帰ハンドルの型エイリアス
- `block_and_expr()`：`Recursive::declare()` で両ハンドルを作り，`block::block_def` と `expr::expr_def` を呼んで実体を構築し，`define()` で結ぶ

各 `*_def` 関数はこれらのハンドルを引数で受ける．

- `block_def(block, expr)`：block と val 内の expr を再帰参照
- `expr_def(block, expr)`：expr の自己再帰と block 式
- `reg_def(expr)` / `mem_def(expr)` / `val_decl_def(expr)`：初期化式
- `fn_def(block, expr)`：関数本体は block か単一式
- `always_def(block)` / `initial_def(block)`：ブロック本体
- `stage_def(expr)` / `val_or_instance_def(expr)`

`fields_def()` は内部で `block_and_expr()` を呼び，必要な子に `.clone()` で配る．`item_def` / `module_def` / `trait_def` は再帰ハンドルを意識せず `fields_def()` を使うだけ．

### 既知の未解決事項

- `parser2` (旧実装) は `Type` / `Stmt` の型不整合を抱えたまま．段階的に移行中．
- `expr_def` は `todo()` スタブ．シグネチャと配線のみ確定．

## ライティングスタイル

- 句読点は `，` と `．` を用いる (`、`, `。` は禁止)．
- `：`, `（`, `）` は使わない．
- コードコメントは後での変更に備え，本パーサーのみ都度挿入することとする．
