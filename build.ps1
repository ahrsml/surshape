# SURSHAPE · por @ahrsml
# Compila la aplicación en release con el Rust LOCAL y arma la distribución en
# .\dist\ y el paquete .\paquetes\SURSHAPE-<versión>-win64.zip. No instala nada
# en el sistema.
#
#   Uso (desde esta carpeta):   powershell -ExecutionPolicy Bypass -File .\build.ps1
#   -Test      corre antes todos los tests (workspace, i18n y noisegek-dsp).
#   -SinZip    arma dist\ pero no el .zip ni el código fuente empaquetado.
#
# Contenido de la distribución:
#   SURSHAPE.exe, LICENSE, THIRD_PARTY.md, licenses\
#   cdp\            programas de CDP compilados (tools\build_cdp.ps1), si existen
#   manual\         manual HTML en español e inglés (docs\generar_docs.py)
#   traducciones\   copias editables de los textos (ver LEEME.txt)
#   codigo_fuente\  código de SURSHAPE + noisegek-dsp, y de CDP8 (GPL / LGPL)
param([switch]$Test, [switch]$SinZip)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
. "$here\env.ps1"

if ($Test) {
    cargo test --workspace --release
    if ($LASTEXITCODE -ne 0) { throw "Fallaron los tests de SURSHAPE" }
    cargo test -p noisegek-dsp --release
    if ($LASTEXITCODE -ne 0) { throw "Fallaron los tests de noisegek-dsp" }
}

cargo build -p surshape-app --release
if ($LASTEXITCODE -ne 0) { throw "Falló la compilación" }
$version = ((Select-String -Path "$here\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value)

$dist = Join-Path $here "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item "$here\target\release\surshape.exe" (Join-Path $dist "SURSHAPE.exe") -Force
Copy-Item "$here\LICENSE" $dist -Force
Copy-Item "$here\THIRD_PARTY.md" $dist -Force
$lic = Join-Path $dist "licenses"
if (Test-Path $lic) { Remove-Item -Recurse -Force $lic }
Copy-Item -Recurse "$here\licenses" $lic

# CDP compilado desde el código oficial (tools\build_cdp.ps1), si existe.
$cdpBin = Join-Path $here "third_party\cdp-bin"
$cdpDist = Join-Path $dist "cdp"
$commit = ""
if (Test-Path (Join-Path $cdpBin "pvoc.exe")) {
    if (Test-Path $cdpDist) { Remove-Item -Recurse -Force $cdpDist }
    Copy-Item -Recurse $cdpBin $cdpDist
    $commit = (Get-Content (Join-Path $cdpBin "COMMIT.txt") -Raw).Trim()
    Set-Content -Encoding utf8 (Join-Path $cdpDist "LEEME.txt") @"
Programas de CDP (Composers Desktop Project), licencia LGPL-2.1 (ver LICENSE-CDP.txt).
SURSHAPE los ejecuta como programas externos; no forman parte de su código.
Compilados sin modificaciones desde el código fuente oficial:
$commit
Código fuente: https://github.com/ComposersDesktop/CDP8 (copia en ..\codigo_fuente\)
"@
    Write-Host "CDP incluido en: $cdpDist"
} else {
    Write-Host "Aviso: CDP no se incluyó (falta third_party\cdp-bin). Para incluirlo: .\tools\build_cdp.ps1"
}

# Capturas recomprimidas (el modo --captura las escribe sin compresión)
if (Get-Command python -ErrorAction SilentlyContinue) { python "$here\tools\comprimir_png.py" }

# Manual (catálogo exportado desde el código + textos de docs\manual\)
cargo run -q -p check_i18n --release --bin exportar_catalogo -- "$here\docs\catalogo.json"
if ($LASTEXITCODE -ne 0) { throw "No se pudo exportar el catálogo de procesos" }
if (Get-Command python -ErrorAction SilentlyContinue) {
    python "$here\docs\generar_docs.py" (Join-Path $dist "manual")
    if ($LASTEXITCODE -ne 0) { throw "Falló la generación del manual" }
} else {
    Write-Host "Aviso: no se encontró Python; el manual no se regeneró."
}

# Traducciones editables
$tr = Join-Path $dist "traducciones"
New-Item -ItemType Directory -Force $tr | Out-Null
Copy-Item "$here\crates\surshape-i18n\locales\*.lang" $tr -Force
Set-Content -Encoding utf8 (Join-Path $tr "LEEME.txt") @"
Textos de SURSHAPE (clave = texto). Para probar cambios sin recompilar:
copia esta carpeta junto a SURSHAPE.exe con el nombre "locales" y edita
es.lang o en.lang. No cambies lo que va entre llaves {así}.
"@

if (-not $SinZip) {
    # Código fuente (GPL: acompaña al programa)
    $fuente = Join-Path $dist "codigo_fuente"
    if (Test-Path $fuente) { Remove-Item -Recurse -Force $fuente }
    New-Item -ItemType Directory -Force $fuente | Out-Null
    $stage = Join-Path $env:TEMP "surshape_fuente_$PID"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    $raiz = Join-Path $stage "SURSHAPE"
    New-Item -ItemType Directory -Force $raiz | Out-Null
    foreach ($item in @("Cargo.toml", "Cargo.lock", "crates", "tools", "docs", "licenses", "env.ps1", "build.ps1",
                        "ejecutar.ps1", "limpiar.ps1", "README.md", "CLAUDE.md", "THIRD_PARTY.md", "LICENSE", ".gitignore")) {
        Copy-Item -Recurse (Join-Path $here $item) $raiz
    }
    # noisegek-dsp va al lado, como lo espera Cargo.toml (..\NOISEGEK\vst\noisegek-dsp)
    $ng = Join-Path $stage "NOISEGEK\vst"
    New-Item -ItemType Directory -Force $ng | Out-Null
    Copy-Item -Recurse (Join-Path $here "..\NOISEGEK\vst\noisegek-dsp") $ng
    # Manifiesto mínimo del workspace de NOISEGEK: solo noisegek-dsp, con los
    # mismos datos [workspace.package] que hereda.
    $orig = Get-Content (Join-Path $here "..\NOISEGEK\vst\Cargo.toml")
    $i = [array]::IndexOf($orig, "[workspace.package]")
    $pkg = @()
    for ($k = $i; $k -lt $orig.Count -and $orig[$k].Trim() -ne ""; $k++) { $pkg += $orig[$k] }
    $min = @("# Workspace mínimo para compilar SURSHAPE (solo noisegek-dsp).", "[workspace]", 'resolver = "2"', 'members = ["noisegek-dsp"]', "") + $pkg
    [IO.File]::WriteAllLines((Join-Path $ng "Cargo.toml"), $min, (New-Object Text.UTF8Encoding $false))
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath (Join-Path $fuente "SURSHAPE-$version-fuente.zip") -Force
    Remove-Item -Recurse -Force $stage

    # Código de CDP8 (LGPL), sin historial ni compilados
    $cdpSrc = Join-Path $here "third_party\CDP8"
    if (Test-Path $cdpSrc) {
        $stage = Join-Path $env:TEMP "surshape_cdp_$PID"
        if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
        New-Item -ItemType Directory -Force (Join-Path $stage "CDP8") | Out-Null
        Get-ChildItem $cdpSrc -Force | Where-Object { @(".git", "build-surshape", "NewRelease", "BinUpdates") -notcontains $_.Name } |
            ForEach-Object { Copy-Item -Recurse $_.FullName (Join-Path $stage "CDP8") }
        if ($commit) { Set-Content -Encoding utf8 (Join-Path $stage "CDP8\COMMIT.txt") $commit }
        Compress-Archive -Path (Join-Path $stage "CDP8") -DestinationPath (Join-Path $fuente "CDP8-fuente.zip") -Force
        Remove-Item -Recurse -Force $stage
    }

    # Paquete final
    $paq = Join-Path $here "paquetes"
    New-Item -ItemType Directory -Force $paq | Out-Null
    $zip = Join-Path $paq "SURSHAPE-$version-win64.zip"
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path (Join-Path $dist "*") -DestinationPath $zip
    Write-Host ("Paquete: $zip (" + [math]::Round((Get-Item $zip).Length / 1MB, 1) + " MB)")
}

Write-Host ""
Write-Host "Listo. Aplicación en: $dist"
Get-ChildItem $dist | ForEach-Object { Write-Host ("  " + $_.Name) }
