#!/bin/sh
# Compile les shaders GLSL 450 de crates/nb-gfx/shaders/glsl450 en SPIR-V (glslangValidator,
# cible Vulkan 1.3) dans crates/nb-gfx/shaders/spv, puis écrit l'empreinte SHA-256 des sources
# (vérifiée par le test `spirv_is_up_to_date`). Les .spv sont commités : la compilation Rust
# ne demande aucun outil externe.
# Usage (sous Docker) : NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh sh crates/nb-gfx/scripts/compile-shaders.sh
set -e
cd "$(dirname "$0")/../shaders"
for f in glsl450/*.vert glsl450/*.frag; do
  name="$(basename "$f")"
  glslangValidator --quiet -V --target-env vulkan1.3 -Iglsl450 -o "spv/$name.spv" "$f"
  spirv-val --target-env vulkan1.3 "spv/$name.spv"
done
(cd glsl450 && sha256sum *.glsl *.vert *.frag) > spv/SOURCES.sha256
echo "SPIR-V à jour :"; ls spv
