32ビット算術論理演算ユニットALU
---

### (配布ファイル)

| ファイル名               | 説明                                 |
|:-------------------------|:-------------------------------------|
| alu32.fsl                | alu32モジュールFSL記述（雛形）       |
| add32.fsl                | add32モジュールFSL記述（雛形）       |
| test_alu32.fsl           | テストベンチFSL記述                  |
| test_alu32.pat           | テスト用パターンファイル             |
| test_alu32.result.sample | テスト用パターンファイル             |
| Makefile                 | Makefile                             |
| README.md                | (このファイル)                       |

### サブモジュールのFSL記述ファイルについて

`alu32.fsl` で用いるサブモジュールのFSL記述ファイル（例えば，加算器の記述ファイルなど）はこのディレクトリにコピーしておく．
配布ファイルにある `add32.fsl` の中身は空です．
また，Makefile の下記の行に必要なFSL記述のファイルを書いておく

    # FSL記述を増やしたら，下記の行に追加する
    SRCS 	= alu32.fsl add32.fsl test_alu32.fsl


### make の機能

1. FSL記述を作成，編集し，`make` でコンパイル（エラーチェック）
3. `make verilog` でコンパイルしてVerilog HDL記述コードの生成
4. `make vvp` で `test_add32.vvp`を生成
5. `make sim` でシミュレーション
6. `make diff` でシミュレーション結果と比較用結果ファイルを比較

### 不要なファイルの消去

1. `make clean` で掃除
2. `make distclean`で `.vvp`ファイルも消去


