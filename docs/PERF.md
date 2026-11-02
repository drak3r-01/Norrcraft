# Performances (phase 7, volet B)

Contrainte de la phase : **sorties identiques bit à bit**. Aucun format de sommet ou de
sauvegarde n'a changé, et aucune valeur attendue de test n'a été modifiée. Les vecteurs Java de
nb-world, les tests dorés de nb-mesh, le déterminisme de nb-ecs et les rendus dorés de nb-gfx
(lavapipe, `NB_GFX_REQUIRE_GPU=1`) restent verts.

## Méthode

* Banc d'essai sans dépendance : `crates/nb-game/benches/pipeline.rs` (`harness = false`),
  chronométrage `Instant`, **médiane** de `NB_BENCH_ITERS` répétitions (7 par défaut) après un
  échauffement. Graine 12345, grille de 8×8 chunks générés ; lumière et maillage mesurés sur les
  36 chunks intérieurs (voisinage 3×3 complet).

  ```sh
  NB_TARGET_VOL=nb-target-perf sh scripts/cargo-docker.sh bench -p nb-game --bench pipeline
  # filtre : ... --bench pipeline -- gen mesh/ frame
  ```
* Chaque section affiche une **empreinte** (FNV-1a des voxels, des cartes de lumière ou des
  `f32` de sommets), calculée hors chronométrage. Elle doit rester identique avant/après : c'est
  un contrôle bit à bit plus large que les tests (64 chunks, 3 types de monde).
* Profilage : `perf record` (paquet `linux-perf`) dans un conteneur `rust:1 --privileged`,
  lancé sur le binaire du banc (`target/release/deps/pipeline-*`).
* Machine : Docker Desktop (WSL2), 24 cœurs logiques, 23 Go. Le bruit entre exécutions est
  d'environ ±5 à 10 %. Une section dont le code n'a pas changé (`light/world`) a mesuré 512,7
  puis 567,0 µs/chunk : en dessous de 10 %, une différence n'est pas significative.

## Mesures

Deux séries consécutives, même session, même binaire de banc. « Avant » = code de `main`
(phases 0 à 6), « après » = branche `phase7/perf`.

| Section | Avant | Après | Écart | Empreinte |
|---|---:|---:|---:|---|
| `gen/normal` (µs/chunk) | 349,7 | 325,7 | −7 % | `b70463aee96495a7` (inchangée) |
| `gen/flat` | 179,8 | 152,3 | −15 % | `ffc1e762aab7dada` (inchangée) |
| `gen/amplified` | 328,1 | 312,7 | −5 % | `8c16be5f17920ec8` (inchangée) |
| `light/world` (code inchangé) | 512,7 | 567,0 | bruit | `b520ff1b47c6ddad` (inchangée) |
| `light/mesh-volume` | 642,8 | 277,3 | **−57 %** | `dc5d52a50cc065c7` (inchangée) |
| `mesh/world-biomes` (µs/chunk) | 1273,2 | 907,0 | **−29 %** | `2cb7b3bad08a9153` (inchangée) |
| `mesh/uniform-biome` | 1274,7 | 870,7 | −32 % | `cb81544a7eb2b974` (inchangée) |
| `manager/load-r6` (ms, rayon 6, 169 chunks) | 36,8 | 31,8 | −14 % | identique |
| `frame/chunk-manager` (µs/frame, rayon 12) | 31,1 | 31,6 | — | identique |
| `frame/mesh-scheduler` (µs/frame) | 196,2 | 198,6 | — | identique |

Lors des mesures faites commit par commit, la génération avait gagné de 10 à 18 % et le
maillage 35 % (voir les messages de commit). Les deux séries ci-dessus donnent l'ordre de
grandeur à retenir.

## Optimisations retenues

1. **Générateur, réutilisation des colonnes** (`nb-world/src/generator.rs`). La passe colonnes
   calcule déjà le biome et les facteurs rivière/océan. Les passes arbres et végétation
   recalculaient le biome (10 évaluations de bruit 2D par colonne), et les candidats arbres
   recalculaient aussi les deux facteurs. Ce sont les mêmes fonctions pures appelées avec les
   mêmes arguments : les valeurs conservées sont identiques par construction.
2. **Lumière du mailleur, diffusion « active »** (`nb-mesh/src/light.rs`). Le Gauss-Seidel
   (14 balayages au plus) visitait les 52 000 cellules à chaque balayage. Il ne visite plus que
   les cellules diffusantes non saturées, dans le même ordre x → y → z. Il travaille dans un
   volume `u8` bordé de zéros, ce qui supprime les tests de bornes. Pourquoi c'est exact :
   * un voisin à 0 contribue `0 − a < 0 ≤ prev`, et jamais la règle du ciel (`up == 15`), ce
     qui revient à « pas de voisin » ;
   * si toutes les atténuations relues sont ≥ 1, aucune valeur ne dépasse
     `M = max(15, max initial)`. Une cellule à `M` ne change donc plus, et l'ignorer ne modifie
     ni les valeurs lues, ni le drapeau `changed`, ni le nombre de balayages.

   Hors de ce domaine (table exotique où une atténuation > 127 est relue négative), le code
   retombe sur la transcription directe du Java, conservée (`diffuse_reference`). Deux tests
   comparent les deux versions sur 36 voisinages aléatoires : `active_diffusion_matches_reference`
   et `exotic_table_falls_back_and_matches_reference`. Aucun `unsafe` n'a été ajouté.

## Tenté et abandonné (mesuré, sans gain)

* **`floor` logiciel exact** (troncature + correction + `copysign`), pour éviter l'appel libm
  `floor` qui pesait 8 % du profil de génération. Les versions avec et sans branche sont
  exactes au bit près sur 1,2 M valeurs, mais plus lentes : 438 contre 390 µs/chunk. La glibc
  choisit déjà `roundsd` (SSE4.1) via ifunc, et le coût restant est celui de l'appel. Activer
  `target-feature=+sse4.1` aurait changé la cible de compilation : écarté.
* **Réutiliser les tampons de cellules du greedy meshing** (supprimer le `fill` par tranche,
  sortir le tableau de `slice_y`). C'est exact, car la fusion consomme toutes les cellules
  marquées, mais le gain est nul dans le bruit (883 contre 870 µs/chunk).
* **Tables précalculées de visibilité de face (256×256 bits) et d'occulteur AO** dans
  `BlockTable` : pas plus rapide (911 contre 870 µs/chunk). Les accès à `BlockMeshInfo` sont
  déjà en cache L1.

## Profil restant (mailleur, après)

`mesh_chunk` (tranches XZ et blocs spéciaux inlinés) pèse ~39 %, la passe colonne et la
diffusion de lumière ~15 %, `slice_y` ~14 %, `BlockCache::new` ~8 %, `face_ao` ~6 %. Pour la
génération : `Noise::evaluate3` (grottes) ~39 %, le corps des passes ~29 %, `evaluate2` ~18 %
et `floor` ~8 %.

## Non traité, et limites

* **Pas de vrai GPU** : seules des mesures CPU et des tests lavapipe ont été faits. **Aucun
  gain GPU n'est revendiqué.** Les vertex buffers de chunks restent en mémoire hôte
  (`MemoryLocation::CpuToGpu`) sans staging. Un passage en `GpuOnly` + staging ne se mesure pas
  sur lavapipe, où toute la mémoire est hôte, et changerait le chemin d'upload sans preuve :
  non fait.
* **Planification des threads** (`ChunkManager`, `MeshScheduler`) : le coût par frame en
  régime stable est de 31 µs et 199 µs (rayon 12). `manager/load-r6` ne mesure pas de goulot
  propre aux pools. Rien n'a été modifié. Pistes non mesurées : le `MeshScheduler` fait
  plusieurs recherches sous verrou par position active et par frame, et chaque tâche de
  maillage copie les voxels de 9 chunks (`WorldChunks::snapshot`).
* **Lumière du monde** (`nb-world/src/light.rs`, BFS, ~540 µs/chunk) : candidate évidente,
  pas traitée dans cette phase.
* **Allocations par tick (nb-ecs, nb-game)** : pas de banc dédié, pas de modification.
* Les mesures viennent d'une seule machine sous Docker/WSL2. Elles n'ont pas été reproduites
  sur d'autres processeurs.
