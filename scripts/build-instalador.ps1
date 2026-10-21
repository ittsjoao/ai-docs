# Gera o instalador NSIS do screenManual em target/release/bundle/nsis/.
# O mesmo script roda local e no GitHub Actions. O CLI precisa sair antes do bundle:
# o tauri.conf.json o empacota como recurso, ao lado do screenManual.exe.
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
$app = Join-Path $raiz 'crates/app'

# whisper-rs-sys compila o whisper.cpp com cmake; fora do Developer Prompt ele não está no PATH,
# então usa o que vem com o Visual Studio (Build Tools)
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vs = & $vswhere -latest -products * -property installationPath
    $env:PATH = "$vs\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin;$env:PATH"
}

Push-Location $app
try {
    npm ci
    if ($LASTEXITCODE) { throw 'npm ci falhou' }
    npm run build
    if ($LASTEXITCODE) { throw 'tsc falhou' }
} finally { Pop-Location }

Push-Location $raiz
try {
    cargo build --release -p screenmanual-cli
    if ($LASTEXITCODE) { throw 'build do CLI falhou' }
} finally { Pop-Location }

Push-Location $app
try {
    cargo tauri build
    if ($LASTEXITCODE) { throw 'cargo tauri build falhou' }
} finally { Pop-Location }

Get-ChildItem (Join-Path $raiz 'target/release/bundle/nsis/*.exe') | ForEach-Object { $_.FullName }
