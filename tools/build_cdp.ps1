# SURSHAPE · por @ahrsml
# Compila los programas de CDP que usa SURSHAPE desde el código oficial
# (CDP8, LGPL-2.1) fijado a un commit, con el MinGW de MSYS2, y los deja en
# .\third_party\cdp-bin\ junto con su licencia y el commit usado.
# build.ps1 los copia a dist\cdp\ si existen.
#
#   powershell -ExecutionPolicy Bypass -File .\tools\build_cdp.ps1
#   -Todos   compila todos los programas (no solo los que usa SURSHAPE).
#
# No toca el sistema: el PATH de MSYS2 se agrega solo para esta ventana.
# Requisitos: git, CMake, Ninja y MSYS2 MinGW64 (C:\msys64\mingw64).
# Los programas de reproducción/grabación de CDP (paplay, recsf...) necesitan
# portaudio y NO se compilan: SURSHAPE no los usa.
param([switch]$Todos)

$ErrorActionPreference = "Stop"
$Commit = "28bc42c72c1a7cb0fab933acd1c433be958a787b"   # CDP8 main, 2026-06-08
$Repo = "https://github.com/ComposersDesktop/CDP8.git"
$Programas = @("pvoc", "blur", "focus", "hilite", "stretch", "distort", "extend", "combine", "morph",
                "strange", "spec", "modify", "filter", "sndinfo", "housekeep", "submix")

$raiz = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$tp = Join-Path $raiz "third_party"
$src = Join-Path $tp "CDP8"
$build = Join-Path $src "build-surshape"
$bin = Join-Path $tp "cdp-bin"
$mingw = "C:\msys64\mingw64\bin"
if (-not (Test-Path $mingw)) { throw "No se encontró MSYS2 MinGW64 en $mingw" }
$env:PATH = "$mingw;$env:PATH"

New-Item -ItemType Directory -Force $tp | Out-Null
if (-not (Test-Path (Join-Path $src ".git"))) {
    git clone $Repo $src
    if ($LASTEXITCODE -ne 0) { throw "Falló git clone" }
}
git -C $src fetch --quiet origin
git -C $src checkout --quiet $Commit
if ($LASTEXITCODE -ne 0) { throw "No se pudo fijar el commit $Commit" }

# Enlace estático: los .exe no dependen de las DLL de MinGW.
cmake -S $src -B $build -G Ninja -DCMAKE_BUILD_TYPE=Release `
    -DCMAKE_C_COMPILER=gcc -DCMAKE_CXX_COMPILER=g++ `
    "-DCMAKE_EXE_LINKER_FLAGS=-static -static-libgcc" `
    "-DCMAKE_C_FLAGS=-O2 -w -fcommon -std=gnu89"
if ($LASTEXITCODE -ne 0) { throw "Falló la configuración de CMake" }

if ($Todos) {
    cmake --build $build -- -k 0
} else {
    cmake --build $build --target $Programas -- -k 0
}
$fallo = $LASTEXITCODE

New-Item -ItemType Directory -Force $bin | Out-Null
$copiados = 0
# CDP8 deja los ejecutables en NewRelease\ (no en la carpeta de build).
$salida = Join-Path $src "NewRelease"
Get-ChildItem $salida, $build -Recurse -Filter *.exe -ErrorAction SilentlyContinue | ForEach-Object {
    if ($Todos -or $Programas -contains $_.BaseName) {
        Copy-Item $_.FullName $bin -Force
        $copiados++
    }
}
Copy-Item (Join-Path $src "LICENSE") (Join-Path $bin "LICENSE-CDP.txt") -Force
Set-Content -Encoding utf8 (Join-Path $bin "COMMIT.txt") "CDP8 $Repo`r`ncommit $Commit"

$faltan = $Programas | Where-Object { -not (Test-Path (Join-Path $bin "$_.exe")) }
Write-Host ""
Write-Host "Programas en ${bin}: $copiados"
if ($faltan) {
    Write-Host "FALTAN: $($faltan -join ', ')"
    exit 1
}
if ($fallo -ne 0) { Write-Host "Aviso: algunos programas no compilaron (los de SURSHAPE sí)." }
Write-Host "Listo."
