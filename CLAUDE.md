## Graphify (graphe de connaissances du code)

Un serveur MCP `graphify-norrcraft` expose un graphe du code (instantané, hors LLM).

Quand l'utiliser :
- Questions d'architecture, de dépendances, d'impact d'une modification, « où est utilisé X », « comment A atteint B » : interroger Graphify AVANT de lire des fichiers.
- Ne pas l'utiliser pour lire ou modifier le contenu exact d'un fichier : utiliser les outils fichier.

Comment :
1. `graph_stats` une fois par session pour vérifier que le graphe est chargé et sa taille.
2. `query_graph` (question en mots-clés précis : noms de fonctions, types, modules) pour une vue d'ensemble ; garder `token_budget` par défaut (2000).
3. `get_node` / `get_neighbors` pour creuser un symbole précis ; `shortest_path` entre deux concepts ; `god_nodes` pour repérer les modules centraux.
4. Ensuite seulement, ouvrir les fichiers cités par le graphe pour vérifier le code réel.

Limites à respecter :
- Le graphe peut être périmé : si des fichiers cités n'existent plus ou diffèrent, le signaler à l'utilisateur et proposer la mise à jour (`mcp run graphify -Service graphify-cli update /project` depuis `D:\Documents\Programmation\MCP tools`), ne pas la lancer sans accord.
- Les arêtes `INFERRED` / `AMBIGUOUS` sont des hypothèses : le dire dans la réponse, vérifier dans le code.
- Ne pas appeler `list_prs`, `get_pr_impact`, `triage_prs` (indisponibles dans cette installation).
- Ne pas lire les ressources `graphify://...` en entier sans nécessité (taille non bornée).

## Compilation, tests et disque (méthode depuis la session 3.3)

Rust uniquement via Docker (`scripts/cargo-docker.sh`, `scripts/build-windows.sh`, `crates/nb-gfx/scripts/vk-docker.sh`). Ne jamais créer de volume Docker : un **seul cache partagé** pour tous les worktrees, `NB_TARGET_VOL=nb-target-win` (avec `nb-cargo` pour le registre) ; les worktrees sont montés au même chemin `/w` dans le conteneur, le cache profite donc à toutes les branches. Pas de volume `nb-target-<branche>`.

Rythme d'une session :
1. Pendant le développement : `cargo check` et les tests de **la seule crate touchée** (`cargo test -p nb-world --lib weather`). Filtrer par nom, ne pas lancer le workspace.
2. Une seule passe complète juste avant la fusion : `cargo test --workspace --no-fail-fast` puis `cargo clippy --workspace --all-targets`.
3. Pas de build de l'exe release à la fin de chaque session : seulement en fin de phase (`build-windows.sh` → `dist/Norrblock`) ou à la demande de Gabriel.
4. Aperçus PNG (`preview_norrcraft`) seulement si le visuel change ; les copier dans `apercu-norr/<session>/` du dossier principal avant de supprimer le worktree.

Moins d'écritures disque : le profil `dev` n'écrit que `debug = "line-tables-only"` (rien pour les dépendances) ; pas de fichiers temporaires inutiles dans le dépôt (scratchpad pour les scripts) ; pas de `cargo clean`, pas de `docker system prune`.
