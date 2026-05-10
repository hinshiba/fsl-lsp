# CLAUDE.md

目標: 独自ハードウェア記述言語`FSL`のLanguage ServerをRustで実装し，VSCode拡張機能として提供する．

## 構造

```
fsl-parser -> fsl-lexer
fsl-lsp --> fsl-analyzer -> fsl-parser
fsl-formatter --> fsl-parser
```

## 規約

### スタイル

MUST
- 簡潔で短いコードにする.
- 論理的なまとまりごとに空行と要約コメントを挿入.

SHOULD
- 積極的に専用型/ラッパの導入を検討.
- 純粋関数として分離.

### コメント

MUST: ファイル, クラス/インターフェース/構造体, 関数等の宣言には言語の一般的なドキュメントコメント(ex: doxygen)が必要. 
SHOULD: 一般的にはWhat/Whyを書く.

### ドキュメント

MUST: 章番号や段階に番号を付与しない