# Jeu en réseau local (LAN)

Phase 11, ajoutée au jeu d'origine (le Java n'avait pas de multijoueur). Le réseau est
**optionnel** : tant qu'on ne l'active pas, le jeu se comporte exactement comme avant, sans
ouvrir de port ni envoyer de paquet. **Aucun serveur dédié** : l'un des joueurs héberge sa
partie depuis son propre jeu, les autres s'y connectent.

## Pour jouer

### Héberger

1. Lancer le jeu, ouvrir (ou créer) un monde sauvegardé.
2. **Échap → OPEN TO LAN**. Le chat annonce l'adresse à donner aux amis, par exemple
   `192.168.1.20:25570`, et le menu pause la rappelle en grand sous les boutons.
3. Pour arrêter : **Échap → CLOSE LAN** (les invités reçoivent « The host closed the game »).
   Quitter le monde (MAIN MENU) ferme aussi la partie.

Seuls les mondes **sauvegardés** (créés depuis « Play ») peuvent être ouverts au réseau : le monde
décoratif du menu principal n'est jamais partageable. L'hôte continue de jouer normalement ; les
menus (pause, inventaire) **ne figent pas le monde** tant que des amis sont connectés.

### Rejoindre

1. Menu principal → **MULTIPLAYER**.
2. Les parties du réseau local apparaissent d'elles-mêmes dans la liste (nom du monde, hôte,
   nombre de joueurs). Cliquer sur l'une d'elles remplit l'adresse, un double clic la rejoint.
3. Sinon, taper l'adresse de l'hôte (`192.168.1.20`, le port `25570` est ajouté tout seul) et
   appuyer sur **Entrée** ou **JOIN GAME**.
4. Un champ « Your name » donne le nom affiché aux autres ; il est retenu, ainsi que la
   dernière adresse, dans `settings.json` (`playerName`, `lastServer`).

À l'arrivée, un écran « Joining the game… » montre la progression de la réception du terrain,
puis le joueur apparaît au point d'apparition du monde de l'hôte. Pour partir : **Échap → LEAVE
GAME**.

### Pendant la partie

| Geste | Effet |
|---|---|
| **T** | Chat : le texte tapé est envoyé à tous ; une ligne commençant par `/` est une commande |
| **Tab** (maintenu) | Liste des joueurs, avec la latence de chacun |
| **Échap** | Menu pause (le monde continue de tourner) |

Les autres joueurs sont dessinés avec un corps en boîtes colorées (une couleur par joueur).
Les messages système (« Tom joined the game », « Tom left the game ») s'affichent en jaune.
Les commandes (`/time`, `/gamemode`…) sont **réservées à l'hôte** ; un invité reçoit « Only the
host can use commands in a LAN game. » (`/help` reste lisible par tous).

### Pare-feu et réseau

| Usage | Protocole et port par défaut |
|---|---|
| Jeu | **TCP 25570** (si le port est pris, l'hôte en choisit un libre et l'affiche) |
| Découverte des parties | **UDP 25571** (diffusion locale) |

Sous Windows, au premier « OPEN TO LAN », le pare-feu demande d'autoriser le jeu sur les réseaux
**privés** : accepter. Si la liste reste vide (réseau d'invités, VPN, Wi-Fi qui isole les appareils),
taper l'adresse de l'hôte à la main suffit : seule la découverte automatique est alors perdue, le
port TCP seul est nécessaire.

## Messages d'erreur

| Message | Cause probable |
|---|---|
| Connection refused: is the game still open to LAN at that address? | L'hôte n'a pas ouvert sa partie, ou mauvais port |
| No answer from that address… | Mauvaise adresse, ordinateurs sur deux réseaux différents, pare-feu |
| Incompatible versions (host 1, you 2)… | Les deux jeux n'ont pas la même version du protocole : même compilation des deux côtés |
| This game uses the "norrcraft" universe, which your version of Norrblock does not have… | Le monde de l'hôte est d'un univers que la version de l'invité ne contient pas : mettre à jour |
| Your version of Norrblock has other "…" blocks than the host's… | Même univers, mais les blocs de l'invité diffèrent de ceux de l'hôte (versions différentes) |
| The game is full (8 players) | Limite de 8 joueurs atteinte |
| The host closed the game | L'hôte a fermé la partie ou quitté le monde |
| Connection to the host timed out | Plus de nouvelles de l'hôte depuis 20 s |

## Fonctionnement

```
 joueur invité                                   joueur hôte (serveur intégré)
┌───────────────┐   TCP : State, BlockEdit,   ┌─────────────────────────────┐
│ Game (client) │ ─── Attack, Chat, Ping ───▶ │ Game (monde, mobs, IA)      │
│  monde        │                             │  HostNet : valide, relaie   │
│  « distant »  │ ◀── Chunk, BlockChange, ─── │                             │
└───────────────┘     Snapshot, Chat, Time    └─────────────────────────────┘
        ▲ UDP 25571 : sonde « NBLAN? » / réponse « NBLAN! » (liste des parties)
```

* **L'hôte fait foi pour le monde.** Il génère les chunks, simule les mobs (IA, apparitions,
  dégâts), garde l'heure. Les joueurs distants sont des entités de son ECS (les mobs les voient,
  les poursuivent et les frappent). Les chunks restent chargés autour de chaque joueur.
* **Le client ne génère ni ne simule rien** (monde en mode « distant ») : il reçoit les chunks
  proches (6 chunks tous les 100 ms, du plus proche au plus éloigné, rayon = sa distance de
  rendu), puis chaque bloc modifié. Les mobs lui arrivent à 20 Hz (rayon de 96 blocs) et sont
  lissés à l'affichage. Le client attend d'avoir les chunks voisins avant de simuler, pour ne
  pas tomber dans le vide à l'arrivée.
* **Chaque joueur fait foi pour lui-même** : position, inventaire et créatif/survie sont ceux
  qu'il annonce (réseau de confiance : amis sur le même LAN). L'hôte vérifie en revanche tout
  ce qui touche au monde partagé : identifiant de bloc valide, hauteur dans les limites, portée
  de 12 blocs, chunk chargé (sinon il renvoie la vérité au client), cadence des coups, taille
  des messages (200 caractères). La santé du client est reprise par l'hôte quand un mob le
  blesse.
* **Protocole** : messages TCP encadrés (`longueur u32` + étiquette + champs), un fil de lecture
  et un d'écriture par connexion, relevés une fois par tick. Version `1`, vérifiée à la
  connexion (`Hello` / `Welcome`). Tout est borné (tailles, nombre d'éléments) : un message
  invalide ou trop gros ferme la connexion de son auteur, jamais le jeu de l'hôte.

Code : crate `nb-net` (protocole, transport, découverte, noms) ; `nb-game/src/net/`
(`host.rs`, `client.rs`, `lan.rs`) et `nb-game/src/game/lan.rs` (branchement sur l'état du jeu) ;
`nb-ui/src/screens/multiplayer.rs` (écran) et `lan_overlay.rs` (liste des joueurs, attente du
terrain) ; `nb-world` (`ChunkManager` : mode distant, centres supplémentaires, journal des
modifications) ; `nb-ecs` (`RemotePlayer`, `NetMob`, ordonnancement `Schedule::client`).

## Limites connues

* **Sauvegarde** : seul l'hôte sauvegarde (le monde, et ses propres inventaire et position). Un
  invité qui revient repart avec un inventaire neuf, au point d'apparition ; ce qu'il a bâti
  est dans le monde de l'hôte.
* Parties limitées à **8 joueurs** ; l'hôte doit rester en jeu. Pas de reprise de connexion :
  après une coupure, on rejoint à nouveau.
* Pas de noms flottants au-dessus des avatars (utiliser **Tab**) ni de son 3D des autres joueurs.
* Réseau de confiance : pas de chiffrement ni de mot de passe. Ne pas exposer le port TCP
  sur Internet.
* Les interfaces restent en anglais (comme le reste du jeu) ; cette documentation en français.
* Testé sous Linux en boucle locale (deux jeux dans un processus, 3 tests dans
  `crates/nb-game/tests/lan.rs` + tests des crates concernés). **Non testé sur deux vrais
  ordinateurs ni avec le rendu Vulkan.**

## Pour les développeurs

* `GameOptions::lan_port` (défaut 25570, `0` = port libre) et `lan_discovery_port` (tests :
  plusieurs jeux par processus). `Game::open_to_lan`, `close_lan`, `join_lan`, `cancel_join`,
  `lan_role`, `lan_view` sont publics ; les écrans passent par `UiCommand::{OpenToLan,
  CloseLan, JoinLan, CancelJoin, RefreshLan}`.
* Ajouter un message : variante dans `nb-net/src/proto.rs` (encode, decode, test d'aller-retour),
  traitement dans `host.rs` (`on_message`) ou `client.rs` (`apply`). Changer le format d'un
  message existant impose d'incrémenter `PROTOCOL_VERSION`.
