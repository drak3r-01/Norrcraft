# Contrats figés (fin de phase 0)

Les signatures détaillées sont dans la section « Contrat Rust proposé » de chaque spec
(`nb-world.md`, `nb-mesh.md`, `nb-ecs.md`, `nb-gfx.md`). Ce document fixe ce qui est partagé.
Toute modification passe par l'architecte.

## Dépendances (sens unique)

`nb-core` ← `nb-data` ← `nb-world` ← `nb-mesh`, `nb-ecs` ← `nb-game` ← `nb-app`
`nb-gfx` dépend de `nb-core`/`nb-data` seulement (reçoit un `FrameInput`, jamais `nb-world`).
`nb-ui` dépend de `nb-core`/`nb-data`. Seul `nb-gfx` touche `ash`.

## Types partagés (déjà dans le code, phase 1)

- `nb_core::JavaRandom` : port exact de `java.util.Random`.
- `nb_core::{ChunkPos, BlockPos}` et constantes `CHUNK_SIZE_{X,Y,Z}`.
- Indexation d'un chunk : `x + 16·y + 2048·z` (`Chunk.java`). La doc Java qui annonce
  `x + 16z + 256y` est fausse.
- `nb_data::{BlockRegistry, BlockIds, ParticleRegistry, MobRegistry, GameSettings}` : registres
  immuables passés par référence.

## Écarts du Java corrigés dans les données

- Renne : `aggressorDetectRange` = 10, `aggressorFleeSpeed` = 3 (le fichier `mobs_defaults.json`
  du Java était faux ; le code `ReindeerDefinition` fait foi).
- Un mob personnalisé démarre du renne intégré d'origine (géométrie comprise).

## Décisions à valider par Gabriel avant la phase 2

1. **Monde FLAT** : le Java le recouvre presque entièrement de lave (passe de lave y=1..11).
   Reproduit à l'identique par défaut.
2. **Séparateur décimal des commandes** (`/time`, `/player`…) : le Java dépend de la locale
   (« 1,0 » en français). Proposition : toujours le point.
3. **Ordre d'itération des entités** : émuler l'ordre d'un `HashMap<Integer>` Java
   (voir `nb-ecs.md` B1) pour garder les mêmes comportements. Proposition : oui.
4. **Écarts volontaires proposés** (jamais appliqués sans accord) : compléter
   `treeDensityOverrides` à 8 valeurs (le Java plante sinon) ; corriger la course lumière /
   `setBlock` ; effacer la shadow map à 1.0 quand les ombres sont coupées ; clamp optionnel
   de l'accumulateur 60 Hz ; trier le quadtree par (x, z) pour un ordre de dessin déterministe.
5. **Bogues Java gardés tels quels** (listés dans chaque spec) : seuils d'ombre 1000/2000,
   halo du bloom, eau invisible vue de dessous, fleurs jamais générées, etc.

## Erreurs relevées dans la doc Java (`docs/`)

Ordre réel des systèmes : PlayerInput, Physics, PassiveAI, AggressiveAI, MobSpawn, Health
(`ecs.md` place MobSpawn en 3ᵉ). `rendering.md` : pas de seuil de bloom, blur 10 passes en
demi-résolution, `cloud.vert/frag` jamais compilés.

## Décisions de Gabriel (2026-11-01)

1. Monde FLAT : **ne pas** reproduire le recouvrement quasi total de lave ; corriger pour un
   monde plat sensé (écart volontaire, documenté et testé).
2. Séparateur décimal des commandes : **toujours le point**, uniforme.
3. Ordre d'itération des entités : libre choix ; on prend un ordre **déterministe** (par id
   d'entité croissant) plutôt que d'émuler le `HashMap` Java.
4. Écarts proposés au §« Décisions » point 4 : **tous appliqués** (au mieux, documentés).
