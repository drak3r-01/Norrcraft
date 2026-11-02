# Parité fonctionnelle avec le jeu Java

Inventaire des fonctionnalités du jeu Java de référence (`drak3r-01/UnrealEngine6ProMax`,
`app/src/main/java/com/engine/core/**`, shaders `app/src/main/resources/shaders`, docs
`docs/*.md`) et de leur état dans la réécriture Rust. Relevé fait en relisant le Java, les
spécifications `docs/spec/*.md` et le code Rust (phase 7, 2026-11-02).

Légende :

* ✅ implémenté et couvert par un test automatique (test CPU, ou test GPU sous lavapipe pour
  le rendu) ;
* ⚠️ implémenté, mais non vérifié par un test ciblé ou jamais regardé sur un vrai GPU
  (seulement rendu logiciel lavapipe sous Docker) ;
* ❌ manquant.

Les tests sont désignés `crate:fichier::test`. Les tests GPU (`nb-gfx/tests/render.rs`,
`nb-gfx/src/ui_pass.rs`, `nb-app/src/driver.rs`) sont sautés sans pilote Vulkan ; ils
s'exécutent sous lavapipe avec `NB_GFX_REQUIRE_GPU=1 crates/nb-gfx/scripts/vk-docker.sh test
--workspace`. Le scénario de bout en bout (`crates/nb-app/scripts/scenario.txt`, lancé par
`crates/nb-app/scripts/e2e.sh` et, depuis le paquet distribuable, par
`scripts/test-dist-linux.sh`) traverse tous les écrans hors écran puis en fenêtre réelle
(Xvfb) avec les couches de validation Vulkan.

**Bilan : aucune fonctionnalité de jeu du Java n'est manquante.** Les seuls éléments Java non
repris sont hors jeu (installeur jpackage/WiX, remplacé par le dossier `dist/Norrblock/` ;
shaders `cloud.vert/frag`, code mort jamais compilé en Java ; import des sauvegardes Java,
exclu par décision). Les points ⚠️ concernent surtout l'aspect visuel sur un vrai GPU et le
comportement réel de la fenêtre Windows (plein écran, VSync, capture de la souris), qui n'ont
pu être vérifiés que sous lavapipe/Xvfb.

## Monde et génération

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| `java.util.Random` (LCG exact, `nextInt/Long/Float/Double`) | ✅ | nb-core | `nb-core:random::seed_42_next_int`, `seed_0_next_long_and_double`, `seed_42_bounded` |
| Bruit Perlin « improved », fBm renormalisé, `hashPosition`, `String.hashCode` | ✅ | nb-world | `nb-world:noise::*` (vecteurs du Java) |
| 8 biomes (PLAINS…MOUNTAINS), sélection, configs, lignes de neige | ✅ | nb-world | `nb-world:biome::reference_biomes_seed_12345`, `snow_lines` |
| Hauteur de surface, océans, rivières, côtes | ✅ | nb-world | `nb-world:tests/reference::surface_heights_seed_12345` |
| Passes de génération : remplissage, grottes, minerais, lave, arbres (6 types), végétation, champignons | ✅ | nb-world | `nb-world:tests/reference::normal_seed_12345_chunks_counts_and_light`, `normal_other_seeds`, `generation_is_deterministic_and_seed_dependent` (chunks comparés aux empreintes produites par le Java) |
| Presets NORMAL / FLAT / AMPLIFIED / ISLAND | ✅ | nb-world | `nb-world:config::presets`, `tests/reference::amplified_all_seeds`, `island_all_seeds`, `flat_java_mode_matches_reference`, `flat_corrected_has_no_lava` (FLAT corrigé, voir DIFFERENCES.md) |
| Paramètres avancés `WorldConfig` (édités à la création, persistés) | ✅ | nb-world, nb-ui | `nb-world:config::roundtrip_skips_non_persisted_fields`, `tolerant_json_and_override_padding` ; `nb-ui:world_select_tests::advanced_panel`, `apply_round_trip` |
| Chunks 16×128×16, index `x + 16y + 2048z`, drapeaux | ✅ | nb-core, nb-world | `nb-core:math::index_matches_java_layout`, `nb-world:chunk::*` |
| Chargement / déchargement asynchrones autour du joueur (threads de génération et de lumière) | ✅ | nb-world | `nb-world:tests/manager::*` |
| Lumière (ciel + lave, BFS, halo de 2 blocs) | ✅ | nb-world | `nb-world:light::*`, `tests/manager::loads_square_and_lights_like_reference` |
| Point d'apparition (recherche de surface, `/setworldspawn`) | ✅ | nb-world, nb-ecs | `nb-world:tests/manager::world_spawn_and_save_roundtrip`, `nb-ecs:tests/commands::setworldspawn_and_kill` |
| Particules ambiantes (feuilles sous les feuillages, gouttes de lave) | ✅ | nb-ecs | `nb-ecs:particles::ambient_emission_is_deterministic_and_located` |

## Blocs

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| 35 blocs de `blocks.json` (ids, propriétés, textures par face, teintes, émission, atténuation) | ✅ | nb-data | `nb-data:blocks::*`, `nb-data:tests/assets::every_block_texture_exists_in_default_pack` |
| Casser / poser (raycast à pas 0,05, voisin, refus dans le corps du joueur) | ✅ | nb-ecs | `nb-ecs:tests/interaction::*` |

## Maillage et culling

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Greedy meshing (tranches Y puis latérales, critère de fusion biome compris) | ✅ | nb-mesh | `nb-mesh:tests/golden::*` (égalité binaire avec des buffers produits par le Java), `tests/mesher::*` |
| Occlusion ambiante, lumière lissée par coin, lumière locale recalculée | ✅ | nb-mesh | `nb-mesh:tests/mesher::ambient_occlusion_darkens_corner`, `light_volume_lava_and_darkness`, golden |
| Fluides (dessus abaissés, faces de surface), croix, nénuphar, feuilles, cactus | ✅ | nb-mesh | `nb-mesh:tests/mesher::fluid_*`, `specials_come_last_with_their_encodings`, `leaves_keep_internal_faces` |
| Teintes de biome (herbe, feuilles, eau) | ✅ | nb-mesh, nb-game | `nb-mesh:tests/mesher::biome_border_splits_merging`, `nb-game:chunks::biomes_match_world_system` |
| Planificateur de remaillage (voisins prêts, générations, résultat vide ignoré) | ✅ | nb-mesh, nb-game | `nb-mesh:scheduler::*`, `nb-game:chunks::scheduler_meshes_world_chunks` |
| Quadtree de chunks + frustum (JOML `testAab`) | ✅ | nb-mesh | `nb-mesh:tests/culling::*` |
| Contour du bloc visé (`HighlightMesh`) | ✅ | nb-mesh | `nb-mesh:highlight::twelve_axis_aligned_segments` |

## ECS, joueur, physique

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| `EntityWorld`, ordre des systèmes (PlayerInput, Physics, PassiveAI, AggressiveAI, MobSpawn, Health) | ✅ | nb-ecs | `nb-ecs:world::*`, `tests/determinism::systems_run_in_real_java_order` |
| Déplacements, sprint, saut, FOV de sprint | ✅ | nb-ecs | `nb-ecs:tests/physics::walking_and_sprinting_speeds`, `jump_height_is_about_one_block_and_repeats_while_held`, `fov_becomes_nan_with_zero_sprint_speed` |
| Vol en créatif (double appui sur Saut, montée/descente) | ✅ | nb-ecs | `nb-ecs:tests/physics::double_tap_toggles_flight_in_creative_only` |
| Collisions, gravité, nage, nénuphar, sol virtuel des chunks non chargés | ✅ | nb-ecs | `nb-ecs:tests/physics::*` |
| Dégâts de chute, lave, mort et réapparition (inventaire vidé) | ✅ | nb-ecs | `nb-ecs:tests/physics::fall_damage_depends_on_mode_and_setting`, `lava_damages_every_half_second_in_survival`, `fatal_fall_respawns_player_with_empty_inventory` |
| Combat : coup sur un mob, recul, fuite, mort | ✅ | nb-ecs | `nb-ecs:tests/interaction::hitting_a_passive_mob_damages_flees_and_knocks_back`, `killing_blow_destroys_mob_in_same_tick_and_aggressors_do_not_flee` |
| Séparation des entités | ✅ | nb-ecs | `nb-ecs:tests/physics::push_apart_splits_overlap_and_player_is_immovable`, `orca_and_fish_are_never_pushed_apart` |
| Boucle fixe 60 Hz (accumulateur) | ✅ | nb-game | `nb-game:fixed_step::*`, `tests/game::fixed_step_drives_game_ticks` |
| Entrées : actions configurables, 1–9, molette, saisie de texte, réaffectation | ✅ | nb-game, nb-app | `nb-game:input::*`, `nb-app:keymap::*` |

## Modes de jeu

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Survie / Créatif par monde, triche autorisée ou non | ✅ | nb-ecs, nb-ui | `nb-ecs:components::game_mode_names`, `tests/commands::gamemode_commands`, `cheats_disabled_blocks_everything_but_help`, `nb-ui:world_select_tests::create_flow_keeps_java_seed_bug` |
| Différences de mode (dégâts, vol, cœurs du HUD en survie seulement) | ✅ | nb-ecs, nb-ui | `nb-ecs:tests/physics::fall_damage_depends_on_mode_and_setting`, `nb-ui:hud::draw_lists_icons_counts_and_vitals` |

## Mobs

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Registre intégré (renne, poisson, orque) + `saves/mobs.json` (surcharges, mobs personnalisés) | ✅ | nb-data, nb-game | `nb-data:mobs::*`, `nb-app:driver::renders_menu_ui_only_and_world_frames` (mob personnalisé lu au démarrage) |
| IA passive (errance, pauses, troupeau, fuite) | ✅ | nb-ecs | `nb-ecs:tests/mobs::passive_*`, `herding_fish_head_towards_the_herd`, `player_flee_lasts_then_pauses` |
| IA agressive (orque : poursuite, morsure, cible survie dans l'eau) | ✅ | nb-ecs | `nb-ecs:tests/mobs::orca_*` |
| Apparition / disparition (surface, colonne d'eau, `maxCount`, distance) | ✅ | nb-ecs | `nb-ecs:tests/mobs::reindeer_spawn_on_surface_after_one_second_and_respect_max_count`, `water_mobs_spawn_mid_column`, `far_mobs_despawn_and_degenerate_configs_do_not_panic` |
| Modèles : formes vanilla, JEM (OptiFine CEM), textures d'entité des packs, cube de repli | ✅ | nb-gfx | `nb-gfx:jem::*`, `nb-gfx:mob_model::*`, `nb-gfx:tests/render::missing_vanilla_and_reloaded_mobs` |
| Écran des mobs (onglets, édition, ajout, suppression, réinitialisation) | ✅ | nb-ui, nb-game | `nb-ui:mobs_tests::*`, `nb-game:tests/game::edited_mobs_are_despawned_when_leaving_mob_settings` |

## Commandes (chat)

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| `/help [cmd]` | ✅ | nb-ecs | `nb-ecs:tests/commands::help_and_dispatch` |
| `/time set <day\|noon\|sunset\|night\|midnight\|sunrise\|n>`, `/time speed <s>` | ✅ | nb-ecs | `nb-ecs:tests/commands::time_commands` |
| `/gamemode <creative\|survival\|1\|0>` | ✅ | nb-ecs | `nb-ecs:tests/commands::gamemode_commands` |
| `/physics gravity <mult>`, `/physics falldamage <on\|off>` | ✅ | nb-ecs | `nb-ecs:tests/commands::physics_commands_affect_player_and_mobs` |
| `/player health\|damage\|walkspeed\|sprintspeed\|flyspeed\|jumpforce <v>` | ✅ | nb-ecs | `nb-ecs:tests/commands::player_commands`, `walk_speed_command_changes_movement` |
| `/default`, `/kill`, `/setworldspawn` | ✅ | nb-ecs | `nb-ecs:tests/commands::default_resets_gameplay_values_but_not_renderer_day_length`, `setworldspawn_and_kill` |
| Chat (T ou `/`, journal 4 lignes, 5 s, commandes avant l'ECS) | ✅ | nb-game, nb-ui | `nb-game:chat::*`, `nb-game:tests/game::chat_runs_commands_before_ecs`, `nb-ui:chat::*` |

## Inventaire

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| 36 cases (barre + grille), empilement, ordre de remplissage, inventaire plein | ✅ | nb-ecs | `nb-ecs:inventory::*` |
| Écran d'inventaire : prendre / poser / fusionner / échanger, objet tenu reposé à la fermeture | ✅ | nb-ecs, nb-ui, nb-game | `nb-ecs:inventory::cursor_*`, `nb-ui:inventory::*`, `nb-game:tests/game::inventory_cursor_and_return_on_close` |
| Jeter (Q détruit un objet) | ✅ | nb-ecs | `nb-ecs:tests/interaction::drop_destroys_one_item_of_selected_slot` |

## Sauvegardes

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Liste des mondes triée, créer, renommer, cloner (recréer), supprimer, noms assainis | ✅ | nb-world, nb-game | `nb-world:save::*`, `nb-game:tests/game::world_list_edit_delete_recreate` |
| Chunks sur disque, réécriture au déchargement, fichier corrompu régénéré | ✅ | nb-world | `nb-world:storage::*`, `tests/manager::unload_saves_and_reload_restores_edits`, `corrupt_chunk_file_is_regenerated` |
| État du joueur (position, angles, santé, mode, vol, inventaire, horloge) | ✅ | nb-game | `nb-game:tests/game::headless_create_play_save_reload` |
| Sauvegarde à la pause → menu et à la fermeture | ✅ | nb-game | `nb-game:tests/game::state_transitions_follow_java`, scénario e2e (`14-menu-after-save`, `16-reloaded`) |
| Import des mondes Java (`world.json` sans version, `.bin`) | — | — | exclu par décision (format versionné sans compatibilité, voir DIFFERENCES.md) |

## Interface et écrans

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Menu principal (logo, PLAY / SETTINGS / QUIT, monde de fond en rotation) | ✅ | nb-ui, nb-game | `nb-ui:menus::main_menu_buttons`, `nb-game:tests/game::starts_in_main_menu_with_rotating_camera` |
| Sélection / création / édition / suppression / recréation de monde, germe, panneau avancé | ✅ | nb-ui | `nb-ui:world_select_tests::*`, `nb-game:tests/screens::full_flow_with_mouse_and_keyboard` |
| Pause (RESUME / SETTINGS / MAIN MENU) | ✅ | nb-ui | `nb-ui:menus::pause_menu` |
| HUD : viseur, barre, sélecteur, icônes, quantités, nom de l'objet (fondu), cœurs et nourriture | ✅ | nb-ui, nb-game | `nb-ui:hud::*`, `nb-game:state::hud_timer` |
| Réglages Graphics (distance, FOV, ombres, bloom, VSync, plein écran, résolution) / Controls (réaffectation, défauts) | ✅ | nb-ui | `nb-ui:settings::*`, `nb-game:tests/screens::key_rebind_goes_through_listener_before_update` |
| Écran des packs de ressources | ✅ | nb-ui | `nb-ui:resource_packs::*` |
| Widgets (boutons, curseurs, champs de texte, cases, listes défilantes, panneaux repliables) | ✅ | nb-ui | `nb-ui:widgets::*`, `nb-ui:common::*` |
| Police `stb_easy_font` (largeurs, segments) | ✅ | nb-ui | `nb-ui:font::*` |
| Rendu GPU de l'interface (quads, icônes de blocs, scissor) | ✅ | nb-gfx | `nb-gfx:ui_pass::renders_quads_icons_and_scissor` (lavapipe), captures e2e |
| Aspect des écrans à l'œil sur un vrai GPU, comparé au Java | ⚠️ | — | captures e2e lavapipe seulement |

## Réglages et fichiers de configuration

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| `saves/settings.json` (format Java, champs absents → défaut, touches GLFW) | ✅ | nb-data | `nb-data:settings::*` |
| Application des réglages (distance, FOV, ombres, bloom, touches) | ✅ | nb-game | `nb-game:tests/screens::ui_only_states_and_world_params`, `nb-ui:settings::toggles_and_apply` |
| VSync, plein écran, taille de fenêtre (effet réel sur la fenêtre) | ⚠️ | nb-app, nb-gfx | `nb-gfx:swapchain::present_mode_policy` ; effet réel non vérifié (Xvfb) |
| `saves/mobs.json` | ✅ | nb-data | `nb-data:mobs::json_roundtrip_keeps_overrides` |

## Packs de ressources

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Recherche de `resource_packs/` (dossier courant, parent, dossier de l'exécutable) | ✅ | nb-gfx | `nb-gfx:packs::packs_root_next_to_binary`, `scripts/test-dist-linux.sh` |
| Liste (Default d'abord), `pack.mcmeta` (parseur minimal), repli sur Default | ✅ | nb-gfx | `nb-gfx:packs::*` |
| Tableau de textures (bandes animées : dernière frame, mips, agrandissement lissé, clé en majuscules) | ✅ | nb-gfx | `nb-gfx:atlas::*` |
| Changement de pack en jeu (atlas, icônes, remaillage complet, modèles de mobs) | ✅ | nb-game, nb-gfx | `nb-game:tests/game::resource_pack_change_invalidates_meshes_and_waits_for_table`, `nb-gfx:tests/render::resize_and_atlas_reload` |

## Rendu

| Fonctionnalité Java | État | Crate | Tests |
|---|---|---|---|
| Terrain éclairé (lumière directionnelle, ambiante, lightmap, gamma) | ✅ | nb-gfx | `nb-gfx:tests/render::lit_terrain_matches_cpu_reference` |
| Cycle jour/nuit (horloge, direction et couleurs du soleil) | ✅ | nb-gfx | `nb-gfx:sky::*`, `tests/render::day_night_cycle_and_fluids` |
| Ciel (dégradé zénith/horizon, soleil) | ✅ | nb-gfx | `nb-gfx:tests/render::sky_matches_cpu_reference` |
| Brouillard | ✅ | nb-gfx | `nb-gfx:tests/render::lit_terrain_matches_cpu_reference` |
| Ombres (LOW / HIGH, OFF = carte effacée à 1.0) | ✅ | nb-gfx | `nb-gfx:frame::shadow_resolutions`, `tests/render::shadows_and_shadow_off_clear` |
| Fluides (passe sans écriture de profondeur, vagues, eau teintée, lave) | ✅ | nb-gfx | `nb-gfx:tests/render::water_and_lava_surfaces` |
| Surbrillance du bloc visé (contour noir) | ✅ | nb-gfx | `nb-gfx:tests/render::highlight_outline_is_black` |
| Mobs (couleur, texturés, transparence) | ✅ | nb-gfx | `nb-gfx:tests/render::color_mobs_match_cpu_reference`, `missing_vanilla_and_reloaded_mobs` |
| Particules (billboards, à l'envers comme en Java) | ✅ | nb-gfx | `nb-gfx:tests/render::particles_billboard_upside_down`, `mob_model::particle_billboard_faces_camera` |
| Culling : quadtree + frustum + requêtes d'occlusion | ✅ | nb-gfx | `nb-gfx:tests/render::culling_and_occlusion_queries` |
| Bloom (flou 10 passes demi-résolution, halo de brouillard) | ⚠️ | nb-gfx | `nb-gfx:tests/render::demo_world_noon_golden` (référence de non-régression produite par Rust, pas par le Java) |
| Nuages volumétriques (marche de rayons dans `pp.frag`), aurores, halo du soleil | ⚠️ | nb-gfx | `nb-gfx:vk/scene::builtin_clouds_decode` ; aspect non comparé au Java |
| Distorsion sous l'eau / dans la lave, distorsion de chaleur près de la lave | ⚠️ | nb-gfx | `nb-gfx:sky::heat_curve`, `tests/render::day_night_cycle_and_fluids` (couleur moyenne seulement) |
| Balancement des feuilles et des plantes | ⚠️ | nb-gfx | shader porté ligne à ligne ; pas de test d'animation |
| Fenêtre : titre, icône, redimensionnement, capture de la souris | ⚠️ | nb-app, nb-gfx | `nb-gfx:swapchain::zero_extent_is_skipped` ; e2e fenêtré sous Xvfb ; non vérifié sous Windows |
| Exécutable Windows (`norrblock.exe`, MinGW) | ⚠️ | nb-app | scénario complet hors écran et fenêtré sous **Wine** + lavapipe (`scripts/test-dist-windows-wine.sh`) ; jamais lancé sur un vrai Windows |
| Rendu sur un vrai GPU (NVIDIA / AMD / Intel) | ⚠️ | nb-gfx | jamais exécuté : seulement lavapipe (rendu logiciel, Docker) |
