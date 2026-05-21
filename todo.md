## util

copy-bin の警告の削減
esbuild のcjs化
fsl-playground の修理

## 0.3.0目標

- 型推論によるインレイヒント

- val newによる別ファイルのモジュールの取得

- 診断機能追加
  - 型の不一致に対するエラー
  - モジュールに対するinput, output制約違反
  - モジュールの実装されている関数
  - モジュールのprivate違反
  
- hover
  - 型情報 特にbitサイズ

- 補完
  - newに対する他モジュールの補完
  - インスタンスに対するoutput, defの補完
  