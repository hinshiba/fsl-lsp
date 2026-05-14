//! ビルトインシンボル
//!
//! `_display` などのシミュレータ組込み名を保持する静的ホワイトリスト．
//! 名前解決時にこのリストにマッチした参照は未宣言エラーから除外する．

/// ビルトイン名の集合
pub struct Builtins {
    names: &'static [&'static str],
}

static BUILTINS: Builtins = Builtins {
    names: &["_display", "_finish", "_time", "_readmemb"],
};

/// グローバル `Builtins` を返す
pub fn builtins() -> &'static Builtins {
    &BUILTINS
}

impl Builtins {
    /// 名前が組込みかを判定する
    pub fn is_builtin(&self, name: &str) -> bool {
        self.names.contains(&name)
    }

    /// 入力名と一致する canonical な組込み名を返す
    pub fn canonical(&self, name: &str) -> Option<&'static str> {
        self.names.iter().copied().find(|n| *n == name)
    }

    /// 全組込み名のスライス
    pub fn all(&self) -> &'static [&'static str] {
        self.names
    }
}
