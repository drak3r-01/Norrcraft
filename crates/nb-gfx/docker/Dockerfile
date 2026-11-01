# Image de test de nb-gfx : toolchain Rust + Vulkan logiciel (lavapipe) + glslang.
# Construite par crates/nb-gfx/scripts/vk-docker.sh (tag local `nb-rust-vk`).
FROM rust:1
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      mesa-vulkan-drivers libvulkan1 vulkan-tools vulkan-validationlayers glslang-tools spirv-tools \
 && rm -rf /var/lib/apt/lists/* \
 && rustup toolchain install stable --profile minimal --component rustfmt,clippy && rustup default stable
ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
