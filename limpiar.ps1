# SURSHAPE · por @ahrsml
# Libera espacio: borra los archivos intermedios de compilación (.\target).
# NO toca .\dist, tus sesiones ni el código.
#
#   powershell -ExecutionPolicy Bypass -File .\limpiar.ps1
#   -Todo   además borra el Rust local propio (.\.toolchain), si existe.
#           El de NOISEGEK nunca se toca desde aquí.
param([switch]$Todo)

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
function SizeMB($p) { if (Test-Path $p) { [math]::Round(((Get-ChildItem $p -Recurse -File -Force | Measure-Object Length -Sum).Sum) / 1MB) } else { 0 } }

$t = Join-Path $here "target"
Write-Host "target: $(SizeMB $t) MB"
if (Test-Path $t) { Remove-Item -Recurse -Force $t; Write-Host "  borrado." }

if ($Todo) {
    $tc = Join-Path $here ".toolchain"
    Write-Host ".toolchain: $(SizeMB $tc) MB"
    if (Test-Path $tc) { Remove-Item -Recurse -Force $tc; Write-Host "  borrado." }
}
