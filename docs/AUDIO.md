# Moteur de son (`nb-audio`)

Tout le son de Norrblock est généré par le crate `nb-audio` : aucun fichier audio n'est
embarqué. Les sons sont des **placeholders procéduraux** (synthèse), remplaçables un à un par de
vrais fichiers `.wav`.

## Vue d'ensemble

```
nb-game::audio (AudioDirector)           nb-audio
  événements ECS, pas, biome, heure  →   AudioEngine (handle non bloquant, canal)
  grotte / eau / altitude                      │
                                               ▼
                                         thread « nb-audio »
                                         Mixer::render() → waveOut (Windows)
```

* `AudioEngine` : handle appelé depuis le thread du jeu. Sans périphérique (Linux/CI, mode sans
  tête, `--script`, `--frames`) il est **muet** et ne coûte rien.
* `Mixer` : calcul pur (testable sans carte son). Voix spatialisées, ambiances, musique,
  réverbération, limiteur.
* Sortie : `waveOut` (winmm.dll, présente sur tout Windows ; aucune DLL à embarquer),
  44,1 kHz stéréo 16 bits, 4 tampons de 512 images (≈ 46 ms de latence totale).

## Son 3D

Chaque source positionnée (bloc cassé, pas d'un mob, oiseau lointain…) est spatialisée à chaque
bloc de 256 images, gains interpolés (pas de craquements) :

* atténuation `(1 − d/portée)^1,5 / (1 + 0,08·d)` ; portée 40 blocs par défaut ;
* panoramique à puissance constante d'après l'orientation de la tête (lacet **et** tangage) ;
  les sources à moins de 2 blocs restent centrées ;
* assombrissement (passe-bas de 16 kHz → 1,5 kHz) avec la distance, et davantage dans le dos ;
* envoi vers une réverbération partagée (plus forte en grotte) ;
* 48 voix maximum ; la plus faible est volée si une plus forte arrive ; deux événements
  identiques au même instant ne s'empilent pas.

Sous l'eau : tout passe dans un passe-bas à 700 Hz (fondu 150 ms).

## Mixage et réglages

Réglages → onglet **Audio** (sauvegardés dans `saves/settings.json`) :

| Clé JSON          | Curseur   | Défaut | Concerne |
|-------------------|-----------|--------|----------|
| `masterVolume`    | Master    | 100 %  | tout |
| `musicVolume`     | Music     | 50 %   | musique générative |
| `ambientVolume`   | Ambience  | 80 %   | vent, vagues, grotte, oiseaux, grillons… |
| `effectsVolume`   | Effects   | 100 %  | blocs, pas du joueur, interface |
| `creaturesVolume` | Creatures | 100 %  | cris et pas des mobs, pas des autres joueurs |

Les anciens `settings.json` sans ces clés gardent les défauts. Pas de 5 %.

## Ce qui produit du son

* **Blocs** : casse, pose (position du bloc) ; matériau déduit du nom du bloc
  (`Material::from_block_name` : pierre, terre, herbe, gravier, sable, neige, bois, feuilles,
  plante, verre, tissu, métal, glace, eau).
* **Joueur** : pas (cadence et volume selon marche/course, matériau du sol), atterrissage
  (intensité selon la vitesse de chute), plouf et brasses dans l'eau, cri de douleur, mort,
  réapparition. Pas de pas en vol (créatif).
* **Mobs** : voix par famille (grognement, mugissement, bêlement, gloussement, cri, grondement,
  couinement, coassement, sifflement, cliquetis, chant de cétacé, bulles, gargouillis,
  hennissement — `voice_for_mob`), hauteur selon la taille ; cri aléatoire toutes les 9–26 s
  (5–14 s pour les agressifs), cri de douleur au coup, cri de mort, bruits de pas selon la
  foulée.
* **Interface** : clic, ouverture/fermeture inventaire et pause, case d'inventaire, bruit
  d'établi à la prise d'un objet fabriqué.
* **Ambiances** (par biome, heure, altitude, plafond) : vent (rafales), vagues, drone et gouttes
  de grotte, grillons la nuit, oiseaux le jour (positionnés au hasard autour du joueur),
  coassements, crépitement de lave.
* **Musique** : générative, temps réel (nappes, cloche de boîte à musique, basse). 13 thèmes :
  menu, prairie, forêt, jungle, désert, neige, montagne, océan, marais, grotte, nuit, volcan,
  mystique. En jeu : phrases de 1–2 minutes séparées de 1–3 minutes de silence ; dans les menus :
  continue. Changement de thème après 6 s de stabilité (pas d'aller-retour aux frontières de
  biomes).

## Compatibilité LAN

* Hôte et client lancent chacun leur propre moteur (le son n'est jamais envoyé sur le réseau).
* Les **autres joueurs** (avatars) ont leurs pas, positionnés en 3D.
* Les mobs répliqués chez un client sont traités comme des mobs locaux (cris, pas).
* Limite connue : les blocs cassés/posés par un autre joueur ne font pas encore de bruit côté
  client (le journal d'éditions du monde est consommé par la couche réseau).

## Mettre ses propres sons

Déposer des fichiers `.wav` (PCM 8/16/24/32 bits ou flottant, mono ou stéréo, toute fréquence —
mixés en mono et rééchantillonnés) dans le dossier **`sounds/`** à côté de l'exécutable.
Nom = identifiant du son ; `nom.wav`, `nom_1.wav`, `nom_2.wav`… sont des variantes tirées au
hasard (hauteur légèrement variée à chaque lecture). Un fichier présent remplace **toutes** les
variantes procédurales de ce son.

Identifiants : `break_<matériau>`, `place_<matériau>`, `step_<matériau>`, `land_<matériau>`
(matériaux : stone, dirt, grass, gravel, sand, snow, wood, leaves, plant, glass, cloth, metal,
ice, water), `splash`, `swim`, `hit`, `player_hurt`, `player_death`, `respawn`, `pickup`,
`craft`, `ui_click`, `ui_open`, `ui_close`, `ui_slider`, `drip`, `bird`, `crackle`,
`mob_<voix>_<idle|hurt|death>` (voix : grunt, moo, bleat, cluck, squawk, growl, squeak, croak,
hiss, chitter, whale, bubble, squelch, whinny).

Pour obtenir tous les sons actuels en `.wav` (modèles, ou pour les écouter) :

```bash
NB_TARGET_VOL=nb-target-audio sh scripts/cargo-docker.sh run --release -p nb-audio --example dump -- audio-dump
```

Le dossier `audio-dump/sfx/` contient chaque son et `audio-dump/mix_*.wav` 45 s de musique +
ambiance par thème.

## Tests

`cargo test -p nb-audio` (37 tests : DSP, synthèse de chaque matériau/voix, décodage WAV,
panoramique, atténuation, volumes, sous l'eau, limite de voix, musique, ambiances) et
`cargo test -p nb-game --test audio` (raccord jeu ↔ moteur avec un moteur « enregistreur » :
volumes, clics d'interface, événements de blocs, pas, ambiances, thèmes).
