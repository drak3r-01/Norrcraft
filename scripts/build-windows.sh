#!/bin/sh
# Produit l'exécutable Windows (x86_64-pc-windows-gnu, MinGW-w64) par cross-compilation dans
# Docker — aucune toolchain Rust n'est nécessaire sur la machine — puis assemble
# dist/Norrblock/ (scripts/package.sh).
#
# Usage : scripts/build-windows.sh            (depuis Git Bash sous Windows, ou Linux/macOS)
# Variables :
#   NB_TARGET_VOL   volume Docker du dossier target (défaut nb-target-win)
#   NB_WIN_PROFILE  profil cargo (défaut release)
#   NB_WIN_NO_WINE  1 = ne pas lancer `norrblock.exe --help` sous Wine après la compilation
#
# L'exécutable charge Vulkan dynamiquement (vulkan-1.dll, fourni par le pilote graphique) et
# n'a besoin d'aucune DLL MinGW : la CRT est liée statiquement (+crt-static) et la liste des
# DLL importées est vérifiée ci-dessous.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOCKER_DIR="$ROOT/scripts/docker"
case "$(uname -s)" in
  MINGW*|MSYS*)
    ROOT_D="$(cd "$ROOT" && pwd -W)"; DOCKER_DIR="$(cd "$DOCKER_DIR" && pwd -W)"
    export MSYS_NO_PATHCONV=1 ;;
  *) ROOT_D="$ROOT" ;;
esac
PROFILE="${NB_WIN_PROFILE:-release}"
TRIPLE=x86_64-pc-windows-gnu

docker image inspect nb-rust-win >/dev/null 2>&1 \
  || docker build -t nb-rust-win -f "$DOCKER_DIR/windows-cross.Dockerfile" "$DOCKER_DIR"

mkdir -p "$ROOT/dist/.build"
docker run --rm -v "$ROOT_D:/w" -v nb-cargo:/usr/local/cargo/registry \
  -v "${NB_TARGET_VOL:-nb-target-win}:/w/target" -w /w \
  -e CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C target-feature=+crt-static" \
  -e NB_WIN_NO_WINE="${NB_WIN_NO_WINE:-0}" \
  nb-rust-win sh -c "
set -e
cargo build --locked --profile $PROFILE -p nb-app --target $TRIPLE
EXE=target/$TRIPLE/$PROFILE/norrblock.exe
cp \$EXE dist/.build/norrblock.exe
echo '== DLL importées :'
x86_64-w64-mingw32-objdump -p \$EXE | sed -n 's/^\tDLL Name: //p' | sort -u | tee /tmp/dlls
# Seules des DLL système Windows (et vulkan-1.dll chargée dynamiquement, absente ici).
if grep -viE '^(kernel32|user32|gdi32|advapi32|ntdll|ole32|oleaut32|shell32|comctl32|imm32|uxtheme|dwmapi|msvcrt|ws2_32|bcrypt|bcryptprimitives|userenv|secur32|crypt32|winmm|uiautomationcore|propsys|shlwapi|version|api-ms-win-.*)\.dll$' /tmp/dlls; then
  echo 'DLL non système importée' >&2; exit 1
fi
if [ \"\$NB_WIN_NO_WINE\" != 1 ]; then
  echo '== norrblock.exe --help sous Wine :'
  WINEDEBUG=-all timeout 300 wine \$EXE --help 2>/dev/null | tr -d '\r' | tee /tmp/help
  grep -q -- '--saves' /tmp/help
fi
"
sh "$ROOT/scripts/package.sh" "$ROOT/dist/.build/norrblock.exe" "$ROOT/dist/Norrblock"
