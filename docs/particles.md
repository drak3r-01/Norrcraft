# Moteur de particules (`nb-particles`)

Refonte complète : l'ancien `ParticleManager` de l'ECS (une `Vec<Particle>` clonant son
`ParticleType`, un draw call + push constants par particule) est supprimé.

## Architecture

| Couche | Rôle |
|---|---|
| `nb-particles` (pur CPU) | Pool à capacité fixe (32 768), simulation, collisions, effets, émetteurs d'ambiance, export d'instances GPU |
| `nb-gfx` | Pipeline `particle` : **un seul draw instancié** (6 sommets × N instances), quad construit dans le vertex shader ; atlas de sprites procéduraux (`particle_tex.rs`) |
| `nb-game::fx` | Pont jeu ↔ moteur : événements ECS, suivi du joueur, météo, ambiance, commandes de chat |

### Performances

* Aucune allocation après `ParticleSystem::new` ; suppression par `swap_remove`.
* Le monde n'est interrogé que quand une particule **change de cellule** et seulement si elle a un
  comportement lié aux blocs (collision, eau, lave…). ~30 000 particules : ≈ 1,3 ms par frame
  (mise à jour + export, un cœur, `cargo test -p nb-particles --test engine throughput -- --nocapture`).
* Instance GPU de 32 octets (`GpuParticle`), tampon `HOST_VISIBLE` par frame en vol, culling
  distance + derrière la caméra, fondu de distance.
* Les émetteurs d'ambiance travaillent en *taux* (événements/s) : indépendants de la fréquence
  d'images, coût borné (≈ 40 requêtes monde par frame), bridés quand le pool dépasse 85–90 %.

### Instance GPU (`GpuParticle`, 32 octets)

position `[f32;3]`, taille `f32`, couleur RGBA8, rotation `u16`, couche `u16`, sous-rectangle UV
`[u8;4]` (1/64), drapeaux : `BLOCK_ATLAS` (échantillonne l'atlas des blocs → éclats de blocs),
`EMISSIVE` (écrit dans le tampon de bloom), `VERTICAL` + étirement (pluie, neige).

## Effets (`presets.rs`)

Tout est `const EffectDef` : durée de vie, vitesse, cône, gravité, traînée, balancement, taille
début/fin, couleur début/fin, rotation, comportements (`behav::*`). Catalogue : éclats de bloc,
poussière, copeaux de pas, éclaboussure + écume + bulles, pluie (+ gouttelettes + rides), neige,
feuilles, lucioles, braises, gouttes/éclats de lave, fumée, flammes, étincelles de coup, nuage de
mort de mob.

## Intégration au jeu

* Bloc cassé / posé (`EcsEvent`), coup et mort de mob.
* Pas (foulée de 2 blocs), atterrissage (selon la vitesse d'impact), entrée dans l'eau.
* Ambiance autour du joueur : feuilles sous les feuillages, lave (gouttes, bulles, braises,
  fumée), magma (braises), lucioles de nuit, bulles et particules en suspension sous l'eau.
* Météo : pluie / neige (à l'abri sous les toits via `sky_open`), fondu de 4 s.
* Commandes de chat : `/weather clear|rain|snow`, `/fx <effet> [n]`, `/fx stats`, `/fx clear`.
  Script nb-app : `spawn-particles NOM N` accepte les noms de `presets::by_name`.

## Ajouter un effet

1. Une `const` dans `presets.rs` (+ une entrée dans `by_name`).
2. Nouveau sprite ? Ajouter son nom à `SPRITE_NAMES` (`effect.rs`) et son dessin dans
   `nb-gfx/src/particle_tex.rs` ; l'ordre des noms = l'ordre des couches de l'atlas.
3. `cargo test -p nb-gfx --test preview preview_particles -- --ignored` produit la planche
   `renders/preview_particles_{day,night}.png`.

## Pistes non faites

* Simulation compute (le CPU tient 30 k particules en ~1 ms ; le compute n'apporterait rien avant
  plusieurs centaines de milliers).
* Tri par profondeur (mélange alpha sans tri aujourd'hui) et particules douces (fondu au contact
  de la géométrie, demande l'échantillonnage de la profondeur).
* Lumière par particule (aujourd'hui : lumière du ciel × lumière du lieu du joueur).
