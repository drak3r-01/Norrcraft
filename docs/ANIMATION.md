# Animation des mobs

Tous les mobs (les 3 d'origine, les 42 animaux de la phase 9) et les avatars des joueurs LAN sont
animés par un moteur squelettique réutilisable, **`nb-anim`**, branché au rendu par un skinning dans
le vertex shader et un dessin instancié. Chaque mob a ses propres clips (marche, course, repos,
broutage, regard, réaction aux dégâts, attaque, nage…), calés sur sa morphologie.

## Vue d'ensemble

```
gen_mobs.py ──► assets/vanilla/mob_rigs.json ──► nb-data (MobConfig.rig) ──► nb-anim (Rig compilé)
 (os, clips)                                   │                          │
                                               ▼                          ▼
              nb-gfx : os de chaque boîte    nb-game : MobAnimSystem  Animator (état d'un mob)
              (attribut de sommet)           (vitesse, regard, LOD)   ──► palette de matrices 3×4
                                               │                          │
                                               └──────► MobInstance.pose ◄┘
                                                          │
                                       mob.vert : p' = palette[os] · p, un draw par modèle
```

* **`crates/nb-anim`** (pur CPU, `glam` + `serde`) : description JSON d'un rig (`def`), compilation en
  tables d'échantillons (`rig`), état et évaluation d'un mob (`animator`), rig de l'avatar
  (`humanoid`).
* **`scripts/mob_shapes.py`, `mob_rigs.py`, `mob_recipes.py`** : chaque boîte de la géométrie est
  étiquetée avec son os ; une *recette* par famille (quadrupèdes, oiseaux, poissons et cétacés,
  tortues, méduses, raies, calmars, crabes, scorpions, lézards, grenouilles, hérissons) écrit les
  clips. `scripts/gen_mobs.py` écrit `assets/vanilla/mob_rigs.json` en même temps que la géométrie.
* **`crates/nb-game/src/anim.rs`** : `MobAnimSystem` déduit le mouvement des positions affichées et
  fait avancer les animateurs.
* **`crates/nb-gfx`** : `mob.vert` (peau + instances), `vk/mobs.rs` (palettes, regroupement par
  modèle, culling).

## Ce que fait le moteur

* **Os et peau rigide** : chaque boîte suit un os ; un os tourne autour de son pivot, avec
  translation et échelle ; hiérarchie jusqu'à 32 os (`MAX_BONES`).
* **Clips** : canaux par os, courbes constantes, ondes sinusoïdales (avec harmonique et rebonds) ou
  clés (Catmull-Rom). Les clips sont « cuits » à 30 échantillons/s : l'exécution ne fait que deux
  lectures et un mélange par canal. Clips en boucle, joués une fois, **additifs** (blessure) ou de
  remplacement.
* **Mélange** : couche de base `idle` ↔ démarche lente ↔ démarche rapide (↔ nage), selon la vitesse
  lissée, puis couches superposées (variantes d'idle, actions, clip « en l'air »), puis regard et
  pilotes secondaires. Les os absents d'un clip retombent sur la pose de repos.
* **Marche sans patinage** : une boucle de démarche correspond à `vitesse × durée` blocs parcourus ;
  la phase avance avec la distance réellement parcourue, quelle que soit la vitesse. Les recettes
  calculent l'amplitude des pattes pour que les pieds restent au sol (`4 × longueur de patte ×
  sin(amplitude)` = longueur de pas) et étirent la patte (`1/cos θ`) pendant l'appui.
* **Idles aléatoires** : à l'arrêt, un mob joue de temps en temps une variante (brouter, renifler,
  regarder autour, hurler, picorer, lisser ses plumes, claquer des pinces…), annulée dès qu'il
  repart. Graines différentes par mob : pas de troupeau qui marche au pas.
* **Actions** : `hurt` (santé qui baisse), `attack` (recharge d'attaque d'un hostile qui
  redémarre), `jump`.
* **Regard** : la tête (et le cou) suit le joueur à moins de ~9 blocs, dans des angles limités.
  Les avatars LAN inclinent la tête comme le joueur.
* **Pilotes secondaires** : le corps s'incline dans les virages, la queue balance à l'opposé, la
  tête anticipe, les poissons plongent/remontent avec leur vitesse verticale.

## Performance

* Un `Animator` ne pèse que quelques dizaines d'octets ; le `Rig` compilé est partagé.
* `Animator::update` (état) coûte quelques flottants et tourne chaque frame pour tous les mobs.
  `Animator::evaluate` (pose) n'a lieu que pour les mobs **proches et dans le champ de la caméra**,
  à cadence réduite avec la distance (chaque frame jusqu'à 28 blocs, une sur 2 jusqu'à 56, une sur 4
  jusqu'à 112, jamais au-delà) ; la palette est exprimée dans le repère du modèle, donc réutilisable
  tant que la pose n'est pas recalculée. Aucune allocation par frame.
* Mesure (`cargo test --release -p nb-game anim::tests::bench_400_mobs -- --ignored --nocapture`) :
  **400 mobs autour de la caméra, ~80 µs par frame** pour tout le système d'animation.
* GPU : palettes de tous les mobs de la frame dans un SSBO (une copie par frame en vol), données par
  instance (matrice modèle, lumière, indice de palette) en attributs d'instance ; **un seul `draw`
  par modèle** et par plage (opaque puis translucide) au lieu d'un `draw` + push constants + sets
  par mob. Les mobs hors du frustum sont écartés avant l'envoi. Les modèles sans rig utilisent la
  palette identité placée en tête du tampon.
* Pourquoi pas une évaluation sur GPU ? Les rigs ont 5 à 25 os et quelques centaines de mobs au plus ;
  la hiérarchie coûte moins de 1 µs par mob sur CPU avec le niveau de détail ci-dessus, alors qu'une
  évaluation GPU imposerait de cuire chaque combinaison de mélange (regard, variantes, pilotes) ou
  de dupliquer tout le mélangeur dans un compute shader. Le format de palette actuel n'empêche pas
  d'y venir plus tard.

## Format d'un rig (`assets/vanilla/mob_rigs.json`)

Un objet par identifiant de mob. Unités : degrés, pixels (16 px = 1 bloc), secondes. Le repère est
celui du modèle : museau vers +Z, +Y en haut. Rotation X positive = vers le bas/l'arrière, Y positive
= vers +X, Z positive = le côté +X monte.

```jsonc
{
  "bones":  [ { "name": "body", "parent": "root", "pivot": [0, 12, 0] }, … ],   // parents avant enfants
  "boxes":  [ 3, 3, 0, … ],           // os de chaque boîte de la géométrie, dans l'ordre
  "clips":  { "walk": { "duration": 0.8, "loop": true, "channels": [
                { "bone": "leg_fl", "rx": {"a": 28, "p": 0.0},          // 28·sin(2π(u+0))
                                    "sy": [[0, 1.0], [0.5, 1.15]] } ] } },  // clés [t, valeur]
  "gaits":  [ { "clip": "walk", "speed": 1.2 }, { "clip": "run", "speed": 4.2 } ],
  "swim":   [ … ],                    // démarches dans l'eau
  "idle":   "idle",
  "idles":  [ { "clip": "graze", "weight": 3 } ], "idle_gap": [3.5, 9],
  "actions": { "hurt": "hurt", "attack": "attack" },
  "air":    "air",
  "look":   { "bones": [ {"bone": "head", "weight": 1} ], "yaw": 52, "pitch": 28, "range": 9 },
  "drivers": [ { "bone": "tail", "prop": "ry", "src": "turn", "gain": -9, "max": 28 } ]
}
```

Les courbes d'échelle (`s`, `sx`, `sy`, `sz`) sont des facteurs absolus (1 = repos), y compris dans
les clips additifs. Les références invalides (os, clip, vitesse nulle…) sont refusées à la
compilation avec un message précis (`RigError`) ; un rig invalide laisse le mob statique.

## Ajouter ou modifier une animation

1. Étiqueter les boîtes du mob avec leur os dans `scripts/mob_shapes.py` (`B(..., bone="head",
   pv=(x, y, z))` ; `pv` = pivot de l'os, 1re boîte qui le donne).
2. Régler la recette de sa famille dans `scripts/mob_recipes.py` / `mob_rigs.py`, ou les réglages
   propres au mob dans le dictionnaire `TUNE` de `scripts/gen_mobs.py` (`graze`, `howl`, `rear`,
   `run_style`, `heavy`, `puff`…).
3. Régénérer : `docker run --rm -v "$PWD:/w" -w /w rust:1 python3 scripts/gen_mobs.py`.
4. Vérifier : les tests `nb-data` (`every_builtin_mob_has_a_sound_rig`) compilent chaque rig, le font
   marcher à plusieurs vitesses et exigent des valeurs finies, des membres près du modèle et des
   pieds au sol ; les planches de poses s'obtiennent avec
   `NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh 'NB_ANIM_MOBS=elk,hare NB_ANIM_STATES=marche,graze cargo test -p nb-gfx --test preview preview_animations -- --ignored'`
   (images dans `crates/nb-gfx/renders/anim_<mob>_<état>.png` ; l'état peut être `repos`,
   `marche`, `course` ou le nom d'un clip).

## Limites actuelles

* Les modèles JEM (OptiFine) et les formes vanilla texturées ne sont pas animés : ils restent statiques
  (la palette identité leur est appliquée). Les mobs personnalisés copiés d'un mob intégré héritent de
  son rig.
* L'éclairage par face est calculé au repos : une partie qui tourne garde la luminosité de sa face.
* Les mobs d'un client LAN s'animent d'après leur mouvement répliqué ; l'attaque et la blessure ne
  sont visibles que là où l'ECS les expose (hôte).
