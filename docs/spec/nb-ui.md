# nb-ui — relevé du Java et contrat

Source : `com.engine.core.ui.*`, `com.engine.core.state.*`, `Main`, `InputManager` (dépôt Java,
branche principale). Coordonnées en **pixels fenêtre**, origine en haut à gauche (projection
`ortho(0, w, h, 0)`), aucune mise à l'échelle DPI.

## 1. Rendu Java

| Élément | Java | Notes |
|---|---|---|
| Quad texturé | `UIRenderer.renderRegionWithTint(tex, x, y, w, h, u, v, uw, vh, rgba)` | `color = texel × tint`, blend `SRC_ALPHA / 1-SRC_ALPHA`, pas de profondeur |
| UV | coin haut de l'écran ↔ `v + vh` (convention GL), textures chargées retournées | en espace image (v = 0 en haut du PNG) : `[u, 1-v-vh] → [u+uw, 1-v]` |
| Texte | `ChatRenderer` = **stb_easy_font** (quads unis), `model = translate(x,y)·scale(s)` | 1000 quads max par appel ; largeur = `stb_easy_font_width × s` (ceil entier avant × s) |
| Icône de bloc | `BlockIconRenderer` : quad **plat** de la couche `getTextureIndex(0)` du tableau de blocs | pas de cube 3D malgré la doc ; v = 1 en haut (tableau retourné) → image à l'endroit |
| Échantillonnage | NEAREST, REPEAT (mips générés mais jamais utilisés : MIN = NEAREST) | le blanc 1×1 est complet (1 seul niveau) |
| Scissor | `glScissor` (origine en bas) | converti en rectangle haut-gauche |

Textures UI : `/textures/hud/{crosshair,hotbar,hotbar_selector,inventory}.png`,
`/textures/ui/{button,logo}.png`, `dirt.png` du pack **Default** (toujours, quel que soit le pack
actif), blanc 1×1, cœurs et cuisses 16×16 procéduraux (masques `HEART_MASK`/`FOOD_MASK`, bord
assombri ×0,55, demi-icône = colonnes 8..15 « vides ») : cœur (210,30,30), vide (75,75,75),
nourriture (185,115,40).

## 2. Widgets et layouts

`UIElement(x, y, w, h)` entiers ; `isHovered` inclusif (`x ≤ mx ≤ x+w`). `UIPanel` + layout
appliqué à chaque `addElement` ; clic : tous les enfants survolés reçoivent `onClick` (récursif).
`VerticalLayout(spacing)` : centrage horizontal `px + (pw - w)/2` (division entière), y cumulé.
`UIGridLayout(cols, sx, sy)`. `UITextButton` : texture plein cadre, texte échelle 2 centré
(`y + (h - 18)/2`). `UISlider` : piste 8 px (0,35), remplissage (0,85), poignée 14 px blanche,
`setValueFromMouseX` borné. `UISlot` : surbrillance blanche α 0,4 au survol.

## 3. Écrans (états)

- **MAIN_MENU** : logo 800×200 en `((w-800)/2, h/4-100)`, boutons 400×50 PLAY / SETTINGS / QUIT
  (VerticalLayout 10, panneau `y = h/2`). Caméra tournante (nb-game).
- **PAUSED** : voile noir α 0,6 ; RESUME / SETTINGS / MAIN MENU (panneau `y = h/4`).
- **PLAYING (HUD)** : viseur 32 px centré ; barre 544×64 en bas (−10) ; sélecteur 72 px ; icônes
  36 px ; quantité échelle 1,8 si > 1 ; nom de l'objet (échelle 2,2, 1 s + fondu 0,5 s) à
  `hotbarY - 30` ; en survie 10 cœurs + 10 cuisses (toujours pleines) 22 px, pas 24.
- **INVENTORY** : voile α 0,6, région 176×166/256 de `inventory.png` étirée en 616×581,
  cases 56 px au pas 63 (27 + barre), icônes 44 px, quantité échelle 2,2, pile tenue au curseur.
- **SETTINGS** (Graphics / Controls), **RESOURCE_PACKS**, **MOB_SETTINGS**, **WORLD_SELECT**
  (MAIN / CREATE / EDIT / DELETE / RECREATE) : fond de terre (tuile 32 px, teinte 0,32), bande
  noire α 0,55, listes défilantes (pas = ligne + écart), panneaux « avancés » repliables, boutons
  désactivés à 0,45, onglets inactifs à 0,6.
- **Chat** (`ChatController`, actif dans tous les états) : T ou `/` ouvre (tampon = « / »),
  Entrée exécute, Échap annule ; journal 4 lignes, visible 5 s après fermeture ; boîte 40 px
  en bas à gauche, largeur 60 % ; lignes de 22 px ; commande en blanc, retour en jaune.

## 4. Contrat Rust

nb-ui est **pur CPU** et ne dépend que de nb-core / nb-data (le contrat interdit nb-ecs : les
données d'inventaire/santé arrivent en vues simples). Chaque écran a `update(&UiInput, …) →
Option<Action>` (les effets — sauvegarde, chargement de monde, changement de réglages — sont
faits par nb-game) et `draw(&mut DrawList, …)`. La `DrawList` est une liste de sommets
(pos, uv en espace image, couleur) découpée en lots (texture UI ou couche du tableau de blocs,
scissor). Le rendu GPU est `nb_gfx::ui_pass::UiRenderer`, enregistré dans l'`OverlayPass`.

## 5. Écarts

- Caractères hors ASCII imprimable remplacés par `?` (comportement indéfini en Java/stb).
  **Décision de Gabriel (2026-11-02) : gardé tel quel** (aucune police étendue ; un nom de
  monde « Forêt » s'affiche « For?t », le nom saisi et le dossier restent intacts).
- Date « Last played » formatée en UTC (Java : fuseau local).
- Le tirage « Random » du germe utilise `JavaRandom` initialisé sur l'horloge.
- Bogues conservés : clic « traversant » à l'entrée des écrans qui remettent `mouseWasDown` à
  faux, barre de défilement des réglages ancrée sur `listTop`, titres centrés approximativement
  (`w/2 - 80`, `w/2 - 60`, `len × 6`), Échap pendant une réaffectation qui quitte l'écran.
- [JAVA-BUG] **corrigé** (décision de Gabriel, 2026-11-02) : à la création d'un monde, le Java
  appelle `closeTyping` (qui vide le tampon du germe) avant de le lire → germe saisi ignoré
  (toujours aléatoire). Le germe est désormais lu avant la fermeture :
  `WorldSelectScreen::fix_seed_input` vaut **vrai** par défaut (faux = comportement Java,
  gardé pour les tests).
- Arbres « Auto » : `(float) -0.001` élargi en double vaut −0,0010000000475, donc le Java
  affiche « -0.0010 » et jamais « Auto » ; reproduit.
- Formats `%.nf` : HALF_UP Java, toujours avec un point (décision des commandes).
- Médium AIR : le cycle ne propose que LAND / WATER ; éditer un mob AIR le repasse en LAND
  (comme en Java).
