# SURSHAPE · por @ahrsml
# Compila (release por defecto) y abre la aplicación.
#   powershell -ExecutionPolicy Bypass -File .\ejecutar.ps1
#   -Debug   compila sin optimizar (más rápido de compilar, más lento de usar).
param([switch]$Debug)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
. "$here\env.ps1"
if ($Debug) { cargo run -p surshape-app } else { cargo run -p surshape-app --release }
