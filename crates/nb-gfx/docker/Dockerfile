# Image de test de nb-gfx : toolchain Rust + Vulkan logiciel (lavapipe) + glslang, et Xvfb
# pour le test de fumée de la partie fenêtrée (exemple `viewer`).
# Construite par crates/nb-gfx/scripts/vk-docker.sh (tag local `nb-rust-vk`).
FROM rust:1
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      mesa-vulkan-drivers libvulkan1 vulkan-tools vulkan-validationlayers glslang-tools spirv-tools \
      xvfb xauth libx11-6 libx11-xcb1 libxcursor1 libxrandr2 libxi6 libxkbcommon-x11-0 \
 && rm -rf /var/lib/apt/lists/* \
 && rustup toolchain install stable --profile minimal --component rustfmt,clippy && rustup default stable
ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
