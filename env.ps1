# SURSHAPE · por @ahrsml
# Activa el Rust LOCAL (solo para esta ventana de PowerShell). No modifica el
# PATH del sistema ni el registro. Usa .\.toolchain si existe; si no, el de
# NOISEGEK (..\NOISEGEK\vst\.toolchain), que es el mismo toolchain.
#   Uso:  . .\env.ps1      (con el punto delante)
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$tc = Join-Path $here ".toolchain"
if (-not (Test-Path $tc)) { $tc = Join-Path $here "..\NOISEGEK\vst\.toolchain" }
if (-not (Test-Path $tc)) { throw "No se encontró el Rust local (.toolchain). Ver README.md." }
$tc = (Resolve-Path $tc).Path
$env:RUSTUP_HOME = Join-Path $tc "rustup"
$env:CARGO_HOME  = Join-Path $tc "cargo"
$bin = Join-Path $env:CARGO_HOME "bin"
if (-not ($env:PATH -split ';' | Where-Object { $_ -eq $bin })) { $env:PATH = "$bin;$env:PATH" }
Write-Host "Rust local activo: $(& (Join-Path $bin 'rustc.exe') --version)  ($tc)"
