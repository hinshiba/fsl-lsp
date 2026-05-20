set windows-shell := ["pwsh.exe", "-NoLogo", "-Command"]

ext_code_run := "pnpm --dir extension/fsl-lsp-code run"


copy-bin:
    cargo build -p fsl-ls --release
    {{ ext_code_run }} copy-bin

checks:
    {{ ext_code_run }} check-types
    {{ ext_code_run }} lint

compile: checks
    {{ ext_code_run }} esbuild

compile-release: checks
    {{ ext_code_run }} esbuild --production

pack: copy-bin compile-release
    {{ ext_code_run }} pack

# publish: pack
