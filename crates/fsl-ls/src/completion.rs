//! Completion ハンドラ
//!
//! `instance.` のメンバアクセス文脈を検出した場合は対象モジュールの
//! メンバ候補を返し，それ以外はスコープ内シンボル・継承メンバ・
//! キーワード・組込み型・ビルトイン関数を一括で返す．

use fsl_analyzer::{Member, SymbolKind};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse,
};

use crate::pos::position_to_offset;
use crate::Backend;

impl Backend {
    /// `completion` の本体
    pub async fn handle_completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let docs = self.docs.read().await;
        let doc = docs.get(&uri)?;
        let offset = position_to_offset(&doc.line_index, pos)?;
        let index = self.index.read().await;

        // `receiver.` のメンバアクセスなら対象モジュールのメンバのみ返す
        if let Some((recv_offset, receiver)) = member_access_context(&doc.text, offset) {
            if let Some(members) =
                fsl_analyzer::api::member_completions(&doc.analysis, &index, recv_offset, &receiver)
            {
                return Some(CompletionResponse::Array(
                    members.iter().map(member_to_item).collect(),
                ));
            }
        }

        // 通常補完  スコープ内シンボル・継承メンバ・キーワード等
        let list = fsl_analyzer::api::completions_at(&doc.analysis, &index, offset);
        let mut items: Vec<CompletionItem> = Vec::new();

        // スコープ内シンボル
        for s in &list.symbols {
            items.push(CompletionItem {
                label: s.name.clone(),
                kind: Some(symbol_kind_to_completion(s.kind)),
                detail: s.ty.as_ref().map(|t| t.to_string()),
                ..Default::default()
            });
        }

        // extends / with による継承メンバ
        for m in &list.inherited {
            items.push(member_to_item(m));
        }

        // キーワード
        for kw in list.keywords {
            items.push(CompletionItem {
                label: (*kw).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }

        // 組込み型
        for ty in list.builtin_types {
            items.push(CompletionItem {
                label: (*ty).to_string(),
                kind: Some(CompletionItemKind::CLASS),
                ..Default::default()
            });
        }

        // ビルトイン関数
        for b in list.builtins {
            items.push(CompletionItem {
                label: (*b).to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                ..Default::default()
            });
        }

        Some(CompletionResponse::Array(items))
    }
}

/// `offset` 直前が `receiver.` のメンバアクセス文脈なら
/// `(レシーバ識別子の開始 offset, レシーバ名)` を返す純粋関数
///
/// `a0.out` のような識別子レシーバに加え，`Bit(8).zero` のような
/// `Name(args).` 形式も引数部を読み飛ばして `Name` を取り出す．
/// FSL の識別子は ASCII のみで構成されるためバイト単位で走査する．
fn member_access_context(text: &str, offset: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';

    // 入力途中のメンバ名を読み飛ばす
    let mut i = offset;
    while i > 0 && is_ident(bytes[i - 1]) {
        i -= 1;
    }

    // 直前がドットでなければメンバアクセスではない
    if i == 0 || bytes[i - 1] != b'.' {
        return None;
    }
    let dot = i - 1;

    // `Name(args).` 形式なら括弧の対応を取って引数部を読み飛ばす
    let mut end = dot;
    if end > 0 && bytes[end - 1] == b')' {
        let mut depth = 0usize;
        loop {
            if end == 0 {
                return None;
            }
            end -= 1;
            match bytes[end] {
                b')' => depth += 1,
                b'(' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    // レシーバ識別子
    let mut j = end;
    while j > 0 && is_ident(bytes[j - 1]) {
        j -= 1;
    }
    if j == end {
        return None;
    }

    Some((j, text[j..end].to_string()))
}

#[cfg(test)]
mod tests {
    use super::member_access_context;

    #[test]
    fn detects_identifier_receiver() {
        let text = "a0.out";
        let (off, recv) = member_access_context(text, 5).unwrap();
        assert_eq!(recv, "a0");
        assert_eq!(off, 0);
    }

    #[test]
    fn detects_type_constructor_receiver() {
        let text = "x = Bit(8).zero";
        let (off, recv) = member_access_context(text, text.len()).unwrap();
        assert_eq!(recv, "Bit");
        assert_eq!(&text[off..off + 3], "Bit");
    }

    #[test]
    fn rejects_non_member_context() {
        assert!(member_access_context("count + 1", 9).is_none());
    }
}

/// `Member` を LSP `CompletionItem` に写す
fn member_to_item(m: &Member) -> CompletionItem {
    CompletionItem {
        label: m.name.clone(),
        kind: Some(symbol_kind_to_completion(m.kind)),
        detail: m.ty.as_ref().map(|t| t.to_string()),
        ..Default::default()
    }
}

/// `SymbolKind` を LSP `CompletionItemKind` に写す
fn symbol_kind_to_completion(k: SymbolKind) -> CompletionItemKind {
    match k {
        SymbolKind::Module => CompletionItemKind::MODULE,
        SymbolKind::Trait => CompletionItemKind::INTERFACE,
        SymbolKind::Reg | SymbolKind::Mem | SymbolKind::Output => CompletionItemKind::VARIABLE,
        SymbolKind::Input => CompletionItemKind::VARIABLE,
        SymbolKind::OutputFn | SymbolKind::Fn => CompletionItemKind::FUNCTION,
        SymbolKind::Instance => CompletionItemKind::FIELD,
        SymbolKind::Stage | SymbolKind::State => CompletionItemKind::CLASS,
        SymbolKind::Val => CompletionItemKind::VARIABLE,
        SymbolKind::Param => CompletionItemKind::VARIABLE,
        SymbolKind::Composite => CompletionItemKind::STRUCT,
    }
}
