# Norrblock (Rust)

Réécriture à l'identique, sans aucune nouveauté, du jeu voxel Java *Norrblock* en Rust + Vulkan
(`ash`, `winit`, `glam`, `hecs`, `serde`).

- Plan de référence : voir `docs/` et `docs/spec/` (spécifications comportementales par crate).
- Jeu Java de référence : `drak3r-01/UnrealEngine6ProMax` (branche principale).
- Fidélité : mêmes constantes, mêmes seuils, même ordre de mise à jour ; la génération du monde
  est identique au Java pour une même graine (port exact du LCG `java.util.Random`).
- Sauvegardes : nouveau format versionné (aucune compatibilité avec les anciens mondes).

## Crates

| Crate      | Rôle                                                              |
|------------|-------------------------------------------------------------------|
| `nb-core`  | Random Java, maths, `ChunkPos`, erreurs                            |
| `nb-data`  | Blocs, mobs, particules, réglages, packs (serde, JSON inchangés)   |
| `nb-world` | Chunks, monde, génération, lumière, stockage                       |
| `nb-mesh`  | Greedy meshing, quadtree, culling (sans GPU)                       |
| `nb-ecs`   | Composants, systèmes, physique, IA, commandes                      |
| `nb-gfx`   | Backend Vulkan                                                    |
| `nb-ui`    | Widgets, layouts, texte, chat                                      |
| `nb-game`  | États, boucle fixe, entrées                                        |
| `nb-app`   | Binaire `norrblock`                                                |

## Développement

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Sans installer Rust (Docker)

```bash
scripts/cargo-docker.sh fmt --all -- --check
scripts/cargo-docker.sh clippy --workspace --all-targets -- -D warnings
scripts/cargo-docker.sh test --workspace
```
