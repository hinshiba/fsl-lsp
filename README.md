# fsl-lsp

fsl言語のlspを提供する予定

現時点ではハイライトを提供する拡張機能です

## ワークスペース構成

| crate              | 役割                                       |
| ------------------ | ------------------------------------------ |
| `fsl-lexer`        | `logos` ベースの字句解析器                 |
| `fsl-parser`       | `chumsky` ベースの構文解析器   |
| `fsl-analyzer`     | シンボルテーブル構築と診断生成   |
| `fsl-ls`           | LSP サーバ実装                    |
| `fsl-playground`   | 上記すべての出力を試せる CLI               |

## fsl-playground

各クレートの出力を一括で試すための CLI です．`fsl-sample/` 配下のサンプル，または任意の `.fsl` ファイルを入力にできます．

### サブコマンド

```sh
# 利用可能なサンプル一覧
cargo run -p fsl-playground -- samples

# 字句解析（--strip-trivia でコメント・改行を除去）
cargo run -p fsl-playground -- lex --sample HelloWorld --strip-trivia

# 構文解析（AST を pretty print）
cargo run -p fsl-playground -- parse --sample HelloWorld

# 意味解析（トップレベルシンボルと診断）
cargo run -p fsl-playground -- analyze --sample HelloWorld

# LSP クレートの状態
cargo run -p fsl-playground -- lsp

# 全段階を順に実行
cargo run -p fsl-playground -- all --sample HelloWorld
```

### 入力指定

サンプル名（`-s`/`--sample`）かファイルパスのどちらかを指定します．

```sh
# サンプル名（fsl-sample/ 配下を再帰的に解決．拡張子 .fsl は省略可）
cargo run -p fsl-playground -- parse --sample alu32-main/alu32
cargo run -p fsl-playground -- parse -s HelloWorld

# ファイルパス直接指定
cargo run -p fsl-playground -- parse path/to/file.fsl
```