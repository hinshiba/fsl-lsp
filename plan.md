# FSL Language Server 実装計画

独自ハードウェア記述言語 FSL (Functional and Scalable hardware description Language) の Language Server を Rust で実装し，VSCode 拡張機能として提供する．

## クレート間依存

```
fsl-parser -> fsl-lexer
fsl-lsp --> fsl-analyzer -> fsl-parser
fsl-formatter --> fsl-parser
```

---

## FSL 文法サマリ

チュートリアルから逆算した文法の概要

### トップレベル

```
compilation_unit = item*

item =
    | trait_def
    | module_def

trait_def  = "trait" IDENT "{" trait_item* "}"
module_def = "module" IDENT [extends_clause] [with_clause] "{" module_item* "}"

extends_clause = "extends" IDENT
with_clause    = "with" IDENT ("with" IDENT)*
```

### モジュールアイテム

```
module_item =
    | reg_decl        // reg <name>: <Type>
    | mem_decl        // mem[<Type>] <name>(<size>) [= (...)]
    | input_decl      // input <name>: <Type>
    | output_decl     // output <name>: <Type>
    | output_fn_decl  // output def <name>(...): <Type>
    | instance_decl   // val <name> = new <ModuleName>
    | fn_def          // [private] def <name>(...): <Type> = <body>
    | always_block    // always { ... }
    | initial_block   // initial { ... }
    | stage_def       // stage <name>(<params>) { ... }
    | type_def        // type <Name>(<field>: <Type>, ...)
    | val_decl        // val <name> [: <Type>] = <expr>
```

### 型

```
type =
    | "Unit"
    | "Boolean"
    | "Bit" "(" INT ")"
    | "Int"
    | "String"
    | "Array" "[" type "]"
    | "List" "[" type "]"
    | "(" type ("," type)* ")"
    | IDENT
```

### 式の演算子優先順位

| 優先度 | 演算子                      |
| ------ | --------------------------- |
| 1      | `\|\|`                      |
| 2      | `&&`                        |
| 3      | `==` `!=` `<` `<=` `>` `>=` |
| 4      | `\|` `^`                    |
| 5      | `&`                         |
| 6      | `<<` `>>` `>>>`             |
| 7      | `++` (ビット連結)           |
| 8      | `+` `-`                     |
| 9      | `*`                         |
| 10     | `#` (符号拡張，二項)        |
| 11     | 単項 `~` `!`                |
| 12     | 後置 `(args)` `.ident`      |

後置連鎖は左再帰をイテレーションで除去する:

```
postfix = primary { "." IDENT ["(" args ")"] | "(" args ")" }
```

`x(a)` (1ビット切り出し) と `f(a)` (関数呼び出し) は構文上同形のため，意味解析フェーズで区別する
LSP の Hover/Completion では未解決シンボルに対しても一貫した態度をとる

### 制御構文

```
stmt =
    | val_decl
    | assignment
    | if_expr
    | match_expr
    | par_block
    | any_block
    | alt_block
    | seq_block
    | generate_stmt
    | relay_stmt
    | finish_stmt
    | goto_stmt
    | expr_stmt
```

### ステージ・ステート

```
stage_def = "stage" IDENT "(" params ")" "{" stage_body "}"
stage_body = state_def* | stmt*

state_def = "state" IDENT stmt
```

---

## Analyzer 実装方針

### スコープ解析

モジュールスコープ → 関数スコープ → ブロックスコープの順に構築するサブモジュールインスタンス (`val x = new Mod`) はモジュールスコープに登録する

### シンボルテーブル

```rust
enum SymbolKind {
    Module,
    Trait,
    Register { width: usize },
    Memory { elem_type: Type, size: usize },
    InputPort { ty: Type },
    OutputPort { ty: Type },
    Function { params: Vec<Type>, ret: Type },
    Stage { params: Vec<Type> },
    Instance { module_name: String },
    Val { ty: Type },
}
```

### 診断ポリシー

- **Error** 
  - nfcでコンパイルエラー
  - nfcはコンパイルエラーにしないが，機能しない文法
    - privateの呼び出し
    - 変数，仮引数への再代入
- **Warning**: 
  - 未使用シンボル
- **Information / Hint**: スタイル(セミコロン未挿入など)，非推奨記法

---

## フォーマッタ実装方針

### 設計方針

設定項目を持たない固定スタイルとする

- インデント: スペース2
- 行幅: 100 文字でソフトラップ
- 二項演算子の前後に空白，単項演算子の後ろに空白なし
- ブロックは K&R スタイル
- セミコロン挿入


### パースエラー時の挙動

パースエラーが1つでもあればフォーマット結果を返さない

---

## LSP 機能実装計画

### フェーズ 1(最小構成)

| 機能                           | 説明                                     |
| ------------------------------ | ---------------------------------------- |
| Diagnostics                    | パースエラー + nfc 一次基準の型エラー    |
| Syntax Highlighting (TextMate) | キーワード・演算子・リテラルの基本色付け |
| Document Symbols               | モジュール・関数・ステージ一覧           |

### フェーズ 2

| 機能             | 説明                                                                                                                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Semantic Tokens  | コンテキスト依存色付け`x(a)` の bit-slice vs 関数呼び出し，モジュール名 vs ステージ名 vs trait 名，入力/出力ポート，ステート名などをシンボルテーブルベースで分類 |
| Go to Definition | シンボル・モジュール・ステージの定義ジャンプ                                                                                                                     |
| Hover            | 型情報・ビット幅                                                                                                                                                 |
| Completion       | キーワード + スコープ内シンボル                                                                                                                                  |

TextMate grammar はトークンレベルの粗い色付け，Semantic Tokens はシンボル解決後の精密な色付け，という二段構成にするAnalyzer 起動前や非対応エディタでは TextMate grammar がフォールバックとして機能する

### フェーズ 2.5

| 機能                | 説明                                              |
| ------------------- | ------------------------------------------------- |
| Document Formatting | ファイル全体のフォーマット                        |
| Range Formatting    | 選択範囲のフォーマット                            |
| Format on Save 対応 | capabilities を返すのみ，有効化はクライアント設定 |

### フェーズ 3

| 機能            | 説明                   |
| --------------- | ---------------------- |
| Find References | シンボルの参照箇所一覧 |
| Rename          | シンボルのリネーム     |
| Inlay Hints     | 型推論結果の表示       |

---


## 実装順序

```
Step 0: 準備
        テストコーパス整備(チュートリアル例の自動収集)
        nfc 呼び出し wrapper 整備
        Newline/セミコロン戦略の最終確定

Step 1: fsl-lexer
        logos で全トークン定義
        コメント収集機能(フォーマッタ用)
        ; と Newline を区切りトークンとして保持

Step 2: fsl-parser
        AST 型定義
        chumsky 0.12.0 でパーサ実装
        Pratt parser で式
        エラー回復

Step 3: fsl-analyzer
        シンボルテーブル構築
        スコープ解析
        nfc 一次基準の診断生成

Step 4: fsl-lsp(フェーズ 1)
        Diagnostics・Document Symbols

Step 5: fsl-formatter
        AST → ソース再生成
        コメント再挿入
        冪等性テスト

Step 6: editors/vscode
        拡張機能パッケージ
        TextMate grammar
        LSP クライアント起動

Step 7+: フェーズ 2 / 2.5 / 3 を順次
```

---

## 既知の制約・注意点

- `nfc` (FSL コンパイラ) は Scala 製バイナリのみ提供ソースは参照不可のため，チュートリアル + nfc の挙動を基準とする
- FSL の完全な仕様書は存在しないため，チュートリアルに記載のない構文はユーザーフィードバックで追加する
- フォーマッタの式中コメント保存は MVP では妥協する場合がある