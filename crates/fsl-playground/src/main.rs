//! FSL ツール群を一括で試すための CLI プレイグラウンド．
//!
//! lex / parse / analyze の各段階の出力を確認できる．
//! 入力はファイルパスか，`fsl-sample/` 配下のサンプル名（`--sample`）で指定する．

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use fsl_analyzer::{Diagnostic, Severity, analyze};
use fsl_lexer::{Span, Token, lex, strip_trivia};
use fsl_parser::parse;

#[derive(Parser)]
#[command(
    name = "fsl-playground",
    about = "FSL の lex/parse/analyze を試すための CLI"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 字句解析の結果（トークン列）を表示する
    Lex {
        #[command(flatten)]
        input: InputArgs,
        /// コメント・改行を除去する
        #[arg(long)]
        strip_trivia: bool,
    },
    /// 構文解析の結果（AST）を表示する
    Parse {
        #[command(flatten)]
        input: InputArgs,
    },
    /// 意味解析の結果（シンボルと診断）を表示する
    Analyze {
        #[command(flatten)]
        input: InputArgs,
    },
    /// LSP クレートのスタブ情報を表示する
    Lsp,
    /// すべての段階を順に実行する
    All {
        #[command(flatten)]
        input: InputArgs,
    },
    /// 利用可能なサンプル一覧を表示する
    Samples,
}

#[derive(Args, Clone)]
struct InputArgs {
    /// 入力ファイルパス
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
    /// `fsl-sample/` 配下のサンプル名（例: `HelloWorld`，`alu32-main/alu32`）
    #[arg(long, short = 's', value_name = "NAME")]
    sample: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Command::Lex {
            input,
            strip_trivia: strip,
        } => {
            let (path, src) = load_input(&input)?;
            print_header("LEX", &path);
            run_lex(&src, strip);
        }
        Command::Parse { input } => {
            let (path, src) = load_input(&input)?;
            print_header("PARSE", &path);
            run_parse(&src);
        }
        Command::Analyze { input } => {
            let (path, src) = load_input(&input)?;
            print_header("ANALYZE", &path);
            run_analyze(&src);
        }
        Command::Lsp => {
            run_lsp();
        }
        Command::All { input } => {
            let (path, src) = load_input(&input)?;
            print_header("LEX", &path);
            run_lex(&src, true);
            println!();
            print_header("PARSE", &path);
            run_parse(&src);
            println!();
            print_header("ANALYZE", &path);
            run_analyze(&src);
            println!();
            run_lsp();
        }
        Command::Samples => {
            list_samples()?;
        }
    }
    Ok(())
}

// ============================================================
// 各段階の実行
// ============================================================

fn run_lex(src: &str, strip: bool) {
    let (tokens, errors) = lex(src);
    let tokens = if strip { strip_trivia(tokens) } else { tokens };
    println!("tokens ({}):", tokens.len());
    for (tok, span) in &tokens {
        println!("  {:?} @ {}", tok, fmt_span(span));
    }
    if !errors.is_empty() {
        println!("lex errors ({}):", errors.len());
        for span in &errors {
            println!("  invalid @ {}", fmt_span(span));
        }
    }
}

fn run_parse(src: &str) {
    let (result, lex_errors) = parse(src);
    if !lex_errors.is_empty() {
        println!("lex errors ({}):", lex_errors.len());
        for span in &lex_errors {
            println!("  invalid @ {}", fmt_span(span));
        }
    }
    println!("items ({}):", result.unit.items.len());
    println!("{:#?}", result.unit);
    if !result.errors.is_empty() {
        println!("parse errors ({}):", result.errors.len());
        for e in &result.errors {
            println!("  {} @ {}", e.message, fmt_span(&e.span));
        }
    }
}

fn run_analyze(src: &str) {
    let (parsed, lex_errors) = parse(src);
    if !lex_errors.is_empty() {
        println!("lex errors ({}):", lex_errors.len());
        for span in &lex_errors {
            println!("  invalid @ {}", fmt_span(span));
        }
    }
    if !parsed.errors.is_empty() {
        println!("parse errors ({}):", parsed.errors.len());
        for e in &parsed.errors {
            println!("  {} @ {}", e.message, fmt_span(&e.span));
        }
    }
    let result = analyze(&parsed.unit);
    println!("top-level symbols ({}):", result.top.symbols.len());
    let mut names: Vec<&String> = result.top.symbols.keys().collect();
    names.sort();
    for name in names {
        let sym = &result.top.symbols[name];
        println!(
            "  {} :: {:?} @ {}",
            sym.name,
            sym.kind,
            fmt_span(&sym.def_span)
        );
    }
    println!("diagnostics ({}):", result.diagnostics.len());
    for d in &result.diagnostics {
        println!(
            "  {} {} @ {}",
            severity_tag(d.severity),
            d.message,
            fmt_span(&d.span)
        );
        let _: &Diagnostic = d;
    }
}

fn run_lsp() {
    print_header("LSP", Path::new("(crate stub)"));
    println!(
        "fsl-ls クレートは tower-lsp を依存に持つが，現状サーバ実装は未着手．\n\
         crate version: {}",
        env!("CARGO_PKG_VERSION")
    );
}

// ============================================================
// 入力解決とサンプル
// ============================================================

const SAMPLE_ROOT: &str = "fsl-sample";

fn load_input(args: &InputArgs) -> Result<(PathBuf, String), String> {
    let path = match (&args.file, &args.sample) {
        (Some(_), Some(_)) => {
            return Err("--sample と FILE は同時に指定できません".into());
        }
        (Some(p), None) => p.clone(),
        (None, Some(name)) => resolve_sample(name)?,
        (None, None) => {
            return Err(
                "入力がありません．FILE または --sample <NAME> を指定してください．\n\
                 利用可能なサンプル一覧は `samples` コマンドで確認できます"
                    .into(),
            );
        }
    };
    let src =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    Ok((path, src))
}

fn resolve_sample(name: &str) -> Result<PathBuf, String> {
    let root = workspace_root().join(SAMPLE_ROOT);
    let candidates = [
        root.join(format!("{name}.fsl")),
        root.join(format!("fsl_tutorial_samples-main/{name}.fsl")),
        root.join(format!("alu32-main/{name}.fsl")),
        root.join(format!("mult32-main/{name}.fsl")),
        root.join(name),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "サンプル `{name}` が見つかりません．`samples` コマンドで一覧を確認してください"
    ))
}

fn list_samples() -> Result<(), String> {
    let root = workspace_root().join(SAMPLE_ROOT);
    if !root.is_dir() {
        return Err(format!("{} が存在しません", root.display()));
    }
    println!("サンプル一覧 ({}):", root.display());
    let mut entries = collect_fsl_files(&root);
    entries.sort();
    for path in entries {
        let rel = path.strip_prefix(&root).unwrap_or(&path);
        let name = rel.with_extension("").to_string_lossy().replace('\\', "/");
        println!("  {name}");
    }
    Ok(())
}

fn collect_fsl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(collect_fsl_files(&p));
        } else if p.extension().and_then(|s| s.to_str()) == Some("fsl") {
            out.push(p);
        }
    }
    out
}

/// CARGO_MANIFEST_DIR から `crates/fsl-playground` の2つ上をワークスペースルートとして解決する．
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

// ============================================================
// 表示補助
// ============================================================

fn print_header(stage: &str, path: &Path) {
    println!("=== {stage} :: {} ===", path.display());
}

fn fmt_span(span: &Span) -> String {
    format!("{}..{}", span.start, span.end)
}

fn severity_tag(s: Severity) -> &'static str {
    match s {
        Severity::Error => "[ERROR]",
        Severity::Warning => "[WARN ]",
        Severity::Information => "[INFO ]",
        Severity::Hint => "[HINT ]",
    }
}

// `Token` を import 経路として保持し依存性を明示する
#[allow(dead_code)]
fn _ensure_token_dep(_t: Token) {}
