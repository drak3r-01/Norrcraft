# Compile Norrblock sous Windows et assemble dist\Norrblock\ (même contenu que
# scripts/package.sh).
#
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#
# - Si `cargo` est installé (rustup + Visual Studio Build Tools, voir README) : compilation
#   native (cible MSVC par défaut), profil release.
# - Sinon, si Docker Desktop et Git Bash (`sh`) sont présents : cross-compilation MinGW dans
#   Docker via scripts/build-windows.sh (aucune installation de Rust nécessaire).
# Option : -Docker force la voie Docker même si cargo est installé.
param([switch]$Docker)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($Docker -or $null -eq $cargo) {
    $sh = Get-Command sh -ErrorAction SilentlyContinue
    $docker = Get-Command docker -ErrorAction SilentlyContinue
    if ($null -eq $sh -or $null -eq $docker) {
        Write-Error ("Ni cargo (Rust) ni Docker + Git Bash ne sont disponibles. " +
            "Installer Rust (https://rustup.rs) ou Docker Desktop, voir README.md.")
    }
    & sh (Join-Path $Root 'scripts/build-windows.sh')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    exit 0
}

Push-Location $Root
try {
    & cargo build --locked --release -p nb-app
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$Exe = Join-Path $Root 'target\release\norrblock.exe'
$Dest = Join-Path $Root 'dist\Norrblock'
if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest -Confirm:$false }
New-Item -ItemType Directory -Force (Join-Path $Dest 'saves') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $Dest 'resource_packs') | Out-Null
Copy-Item $Exe (Join-Path $Dest 'norrblock.exe')
Copy-Item -Recurse (Join-Path $Root 'assets') (Join-Path $Dest 'assets')
Copy-Item -Recurse (Join-Path $Root 'resource_packs\Default') (Join-Path $Dest 'resource_packs\Default')
Copy-Item (Join-Path $Root 'assets\settings_defaults.json') (Join-Path $Dest 'saves\settings.json')
Copy-Item (Join-Path $Root 'assets\mobs_defaults.json') (Join-Path $Dest 'saves\mobs_defaults.json')
Copy-Item (Join-Path $Root 'LICENSE') (Join-Path $Dest 'LICENSE')
Copy-Item (Join-Path $Root 'scripts\LISEZMOI.txt') (Join-Path $Dest 'LISEZMOI.txt')

foreach ($f in @('norrblock.exe', 'resource_packs\Default\pack.mcmeta',
                 'resource_packs\Default\assets\minecraft\textures\block', 'saves\settings.json')) {
    if (-not (Test-Path (Join-Path $Dest $f))) { Write-Error "manquant dans le paquet : $f" }
}
Write-Host "paquet pret : $Dest"
Write-Host "lancer : $Dest\norrblock.exe   (ou double-clic dans l Explorateur)"
