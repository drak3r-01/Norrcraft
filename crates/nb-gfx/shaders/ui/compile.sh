#!/bin/sh
# Compile les shaders de l'UI (ui.vert / ui.frag) en SPIR-V et met à jour SOURCES.sha256
# (vérifié par le test `ui_spirv_is_up_to_date` de src/ui_pass.rs).
# Usage : NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh sh crates/nb-gfx/shaders/ui/compile.sh
set -e
cd "$(dirname "$0")"
for f in ui.vert ui.frag; do
  glslangValidator --quiet -V --target-env vulkan1.3 -o "$f.spv" "$f"
  spirv-val --target-env vulkan1.3 "$f.spv"
done
sha256sum ui.vert ui.frag > SOURCES.sha256
echo "SPIR-V UI à jour"
