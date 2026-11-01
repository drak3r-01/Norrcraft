#!/bin/sh
# Test de fumée de la partie fenêtrée : exemple `viewer` sous Xvfb + lavapipe (image
# `nb-rust-vk`), 60 frames avec redimensionnement, bascule vsync et changement d'ombres ;
# échoue s'il y a des erreurs de validation Vulkan.
# Usage : crates/nb-gfx/scripts/smoke-window.sh
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NB_SH=1 exec sh "$HERE/vk-docker.sh" 'cargo build -p nb-gfx --example viewer \
  && NB_VIEWER_FRAMES=60 xvfb-run -a -s "-screen 0 1920x1080x24" timeout 300 target/debug/examples/viewer'
