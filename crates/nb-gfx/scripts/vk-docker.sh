#!/bin/sh
# Lance cargo (ou une commande) dans l'image `nb-rust-vk` : toolchain Rust + Vulkan logiciel
# (lavapipe) + glslang. L'image est construite à la demande depuis crates/nb-gfx/docker.
# Usage : crates/nb-gfx/scripts/vk-docker.sh test -p nb-gfx
#         NB_GFX_REQUIRE_GPU=1 crates/nb-gfx/scripts/vk-docker.sh test -p nb-gfx
#         NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh '<commande shell>'
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
DOCKER_DIR="$HERE/../docker"
case "$(uname -s)" in
  MINGW*|MSYS*) ROOT="$(cd "$ROOT" && pwd -W)"; DOCKER_DIR="$(cd "$DOCKER_DIR" && pwd -W)"; export MSYS_NO_PATHCONV=1 ;;
esac
docker image inspect nb-rust-vk >/dev/null 2>&1 || docker build -t nb-rust-vk "$DOCKER_DIR"
if [ -n "$NB_SH" ]; then
  exec docker run --rm -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry \
    -v "${NB_TARGET_VOL:-nb-target-gfx}:/w/target" -w /w \
    -e NB_GFX_REQUIRE_GPU -e NB_GFX_UPDATE_GOLDEN nb-rust-vk sh -c "$*"
fi
exec docker run --rm -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry \
  -v "${NB_TARGET_VOL:-nb-target-gfx}:/w/target" -w /w \
  -e NB_GFX_REQUIRE_GPU -e NB_GFX_UPDATE_GOLDEN nb-rust-vk cargo "$@"
