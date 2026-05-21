//! 動作確認用: 引数のファイルを整形して標準出力に書き出す。
use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).expect("usage: dump <path>");
    let src = fs::read_to_string(&path).expect("failed to read file");
    match fsl_fmt::format(&src) {
        Some(out) => print!("{}", out),
        None => {
            eprintln!("parse/lex error");
            std::process::exit(1);
        }
    }
}
