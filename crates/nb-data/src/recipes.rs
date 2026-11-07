//! Recettes de craft (`assets/recipes.json`).
//!
//! Trois stations : la **main** (grille 2×2 de l'inventaire), l'**établi** (3×3, accepte aussi
//! les recettes de la main) et la **fonderie** (2×1 : matière + combustible).
//! Une recette est *façonnée* (motif de cases, translatable et symétrique gauche/droite) ou
//! *sans forme* (liste d'ingrédients dans n'importe quel ordre). Un ingrédient est un objet
//! (`"stone"`) ou une étiquette (`"#planks"`) qui regroupe plusieurs objets.

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::Deserialize;

use crate::items::{ItemId, ItemRegistry};

/// Contenu de `assets/recipes.json`, embarqué dans le binaire.
pub const RECIPES_JSON: &str = include_str!("../../../assets/recipes.json");

/// Station de craft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Station {
    /// Grille 2×2 de l'écran d'inventaire.
    Hand,
    /// Bloc `workbench` : grille 3×3.
    Workbench,
    /// Bloc `forge` : grille 2×1 (matière, combustible).
    Forge,
}

impl Station {
    /// Largeur et hauteur de la grille de la station.
    pub fn grid_size(self) -> (usize, usize) {
        match self {
            Station::Hand => (2, 2),
            Station::Workbench => (3, 3),
            Station::Forge => (2, 1),
        }
    }

    /// Les recettes de `recipe_station` sont-elles utilisables à cette station ?
    pub fn accepts(self, recipe_station: Station) -> bool {
        self == recipe_station || (self == Station::Workbench && recipe_station == Station::Hand)
    }
}

/// Un ingrédient : un objet précis ou un ensemble d'objets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ingredient {
    Item(ItemId),
    Tag(Vec<ItemId>),
}

impl Ingredient {
    pub fn matches(&self, item: ItemId) -> bool {
        match self {
            Ingredient::Item(i) => *i == item,
            Ingredient::Tag(v) => v.contains(&item),
        }
    }

    /// Un objet représentatif (affichage / documentation).
    pub fn first_item(&self) -> ItemId {
        match self {
            Ingredient::Item(i) => *i,
            Ingredient::Tag(v) => v[0],
        }
    }
}

/// Forme d'une recette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Motif `width × height`, ligne par ligne ; `None` = case vide.
    Shaped {
        width: usize,
        height: usize,
        cells: Vec<Option<Ingredient>>,
    },
    /// Ingrédients sans ordre.
    Shapeless(Vec<Ingredient>),
}

/// Résultat d'un craft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Output {
    pub item: ItemId,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub station: Station,
    pub shape: Shape,
    pub output: Output,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    item: String,
    #[serde(default = "one")]
    count: i32,
}

fn one() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
struct RawRecipe {
    station: Station,
    result: RawOutput,
    #[serde(default)]
    pattern: Vec<String>,
    #[serde(default)]
    key: HashMap<String, String>,
    #[serde(default)]
    shapeless: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawBook {
    #[serde(default)]
    tags: HashMap<String, Vec<String>>,
    recipes: Vec<RawRecipe>,
}

/// Livre de recettes.
#[derive(Clone, Debug, Default)]
pub struct RecipeBook {
    recipes: Vec<Recipe>,
}

impl RecipeBook {
    /// Livre intégré (`recipes.json`).
    pub fn builtin(items: &ItemRegistry) -> Self {
        Self::from_json(items, RECIPES_JSON).expect("recipes.json embarqué invalide")
    }

    pub fn from_recipes(recipes: Vec<Recipe>) -> Self {
        Self { recipes }
    }

    pub fn from_json(items: &ItemRegistry, json: &str) -> Result<Self> {
        let ctx = "recipes.json";
        let raw: RawBook = serde_json::from_str(json).map_err(|e| Error::data(ctx, e))?;
        let item = |name: &str| -> Result<ItemId> {
            items
                .id_of(name)
                .ok_or_else(|| Error::data(ctx, format!("objet inconnu : {name}")))
        };
        let mut tags: HashMap<&str, Vec<ItemId>> = HashMap::new();
        for (t, names) in &raw.tags {
            let ids = names.iter().map(|n| item(n)).collect::<Result<Vec<_>>>()?;
            if ids.is_empty() {
                return Err(Error::data(ctx, format!("étiquette vide : {t}")));
            }
            tags.insert(t, ids);
        }
        let ingredient = |s: &str| -> Result<Ingredient> {
            match s.strip_prefix('#') {
                Some(t) => tags
                    .get(t)
                    .cloned()
                    .map(Ingredient::Tag)
                    .ok_or_else(|| Error::data(ctx, format!("étiquette inconnue : {t}"))),
                None => item(s).map(Ingredient::Item),
            }
        };
        let mut recipes = Vec::new();
        for r in &raw.recipes {
            let out_name = &r.result.item;
            if r.result.count < 1 || r.result.count > crate::items::STACK_MAX {
                return Err(Error::data(ctx, format!("{out_name} : quantité invalide")));
            }
            let output = Output {
                item: item(out_name)?,
                count: r.result.count,
            };
            let shape = if r.pattern.is_empty() {
                if r.shapeless.is_empty() {
                    return Err(Error::data(ctx, format!("{out_name} : recette vide")));
                }
                Shape::Shapeless(
                    r.shapeless
                        .iter()
                        .map(|s| ingredient(s))
                        .collect::<Result<_>>()?,
                )
            } else {
                let width = r.pattern[0].chars().count();
                if width == 0 || r.pattern.iter().any(|row| row.chars().count() != width) {
                    return Err(Error::data(ctx, format!("{out_name} : motif irrégulier")));
                }
                let mut cells = Vec::new();
                for row in &r.pattern {
                    for ch in row.chars() {
                        if ch == ' ' {
                            cells.push(None);
                        } else {
                            let k = r.key.get(&ch.to_string()).ok_or_else(|| {
                                Error::data(ctx, format!("{out_name} : clé « {ch} » absente"))
                            })?;
                            cells.push(Some(ingredient(k)?));
                        }
                    }
                }
                if cells.iter().all(Option::is_none) {
                    return Err(Error::data(ctx, format!("{out_name} : motif vide")));
                }
                Shape::Shaped {
                    width,
                    height: r.pattern.len(),
                    cells,
                }
            };
            let (gw, gh) = r.station.grid_size();
            let fits = match &shape {
                Shape::Shaped { width, height, .. } => *width <= gw && *height <= gh,
                Shape::Shapeless(v) => v.len() <= gw * gh,
            };
            if !fits {
                return Err(Error::data(
                    ctx,
                    format!("{out_name} : ne tient pas dans la grille de la station"),
                ));
            }
            recipes.push(Recipe {
                station: r.station,
                shape,
                output,
            });
        }
        Ok(Self { recipes })
    }

    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Recettes qui produisent `item` (commande `/recipe`, documentation).
    pub fn producing(&self, item: ItemId) -> impl Iterator<Item = &Recipe> {
        self.recipes.iter().filter(move |r| r.output.item == item)
    }

    /// Première recette de la station qui correspond à la grille `width × height`
    /// (ligne par ligne ; `None` = case vide).
    pub fn find(
        &self,
        station: Station,
        grid: &[Option<ItemId>],
        width: usize,
        height: usize,
    ) -> Option<&Recipe> {
        debug_assert_eq!(grid.len(), width * height);
        let filled = grid.iter().filter(|c| c.is_some()).count();
        if filled == 0 {
            return None;
        }
        // Boîte englobante des cases occupées.
        let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
        for y in 0..height {
            for x in 0..width {
                if grid[y * width + x].is_some() {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        let window = |x: usize, y: usize| grid[(y0 + y) * width + x0 + x];
        self.recipes
            .iter()
            .filter(|r| station.accepts(r.station))
            .find(|r| match &r.shape {
                Shape::Shaped {
                    width: rw,
                    height: rh,
                    cells,
                } => {
                    if *rw != bw || *rh != bh {
                        return false;
                    }
                    let ok = |mirror: bool| {
                        (0..bh).all(|y| {
                            (0..bw).all(|x| {
                                let rx = if mirror { bw - 1 - x } else { x };
                                match (&cells[y * rw + rx], window(x, y)) {
                                    (None, None) => true,
                                    (Some(ing), Some(it)) => ing.matches(it),
                                    _ => false,
                                }
                            })
                        })
                    };
                    ok(false) || ok(true)
                }
                Shape::Shapeless(ings) => {
                    if ings.len() != filled {
                        return false;
                    }
                    let items: Vec<ItemId> = grid.iter().flatten().copied().collect();
                    assign(ings, &items, &mut vec![false; items.len()])
                }
            })
    }
}

/// Affectation un-à-un des ingrédients aux objets (retour arrière : les étiquettes se
/// recouvrent).
fn assign(ings: &[Ingredient], items: &[ItemId], used: &mut Vec<bool>) -> bool {
    let Some((first, rest)) = ings.split_first() else {
        return true;
    };
    for i in 0..items.len() {
        if !used[i] && first.matches(items[i]) {
            used[i] = true;
            if assign(rest, items, used) {
                return true;
            }
            used[i] = false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::BlockRegistry;

    fn setup() -> (ItemRegistry, RecipeBook) {
        let b = BlockRegistry::builtin_with_craft();
        let i = ItemRegistry::builtin(&b);
        let r = RecipeBook::builtin(&i);
        (i, r)
    }

    fn g(items: &ItemRegistry, names: &[&str]) -> Vec<Option<ItemId>> {
        names
            .iter()
            .map(|n| (!n.is_empty()).then(|| items.id_of(n).unwrap()))
            .collect()
    }

    #[test]
    fn builtin_book_loads() {
        let (_, r) = setup();
        assert!(r.len() >= 30);
    }

    #[test]
    fn shaped_recipe_is_translatable() {
        let (i, r) = setup();
        let stick = i.id_of("stick").unwrap();
        // 2 planches l'une sur l'autre : en haut à gauche, à droite, ou dans un établi 3×3.
        for (names, w, h) in [
            (vec!["planks_oak", "", "planks_oak", ""], 2, 2),
            (vec!["", "planks_birch", "", "planks_oak"], 2, 2),
            (
                vec!["", "", "", "", "planks_acacia", "", "", "planks_acacia", ""],
                3,
                3,
            ),
        ] {
            let grid = g(&i, &names);
            let rec = r.find(Station::Workbench, &grid, w, h).expect("bâtons");
            assert_eq!(rec.output.item, stick);
            assert_eq!(rec.output.count, 4);
        }
        // Diagonale : pas la recette.
        let grid = g(&i, &["planks_oak", "", "", "planks_oak"]);
        assert!(r.find(Station::Hand, &grid, 2, 2).is_none());
    }

    #[test]
    fn mirrored_pattern_matches() {
        let (i, r) = setup();
        // Forge : matière à droite, combustible à gauche = motif inversé.
        let grid = g(&i, &["coal", "iron_ore"]);
        let rec = r.find(Station::Forge, &grid, 2, 1).expect("lingot");
        assert_eq!(rec.output.item, i.id_of("iron_ingot").unwrap());
    }

    #[test]
    fn shapeless_any_order() {
        let (i, r) = setup();
        let a = g(&i, &["stone_bricks", "moss", "", "", "", "", "", "", ""]);
        let b = g(&i, &["", "", "", "", "moss", "", "", "", "stone_bricks"]);
        for grid in [a, b] {
            let rec = r.find(Station::Workbench, &grid, 3, 3).expect("moussu");
            assert_eq!(rec.output.item, i.id_of("mossy_stone_bricks").unwrap());
        }
    }

    #[test]
    fn tags_match_any_wood_but_outputs_differ() {
        let (i, r) = setup();
        let birch = g(&i, &["birch_wood", "", "", ""]);
        let rec = r.find(Station::Hand, &birch, 2, 2).unwrap();
        assert_eq!(rec.output.item, i.id_of("planks_birch").unwrap());
        let mixed = g(
            &i,
            &["planks_oak", "planks_cherry", "planks_birch", "planks_oak"],
        );
        let rec = r.find(Station::Hand, &mixed, 2, 2).unwrap();
        assert_eq!(rec.output.item, i.id_of("workbench").unwrap());
    }

    #[test]
    fn stations_are_isolated() {
        let (i, r) = setup();
        let ring = g(
            &i,
            &[
                "stone", "stone", "stone", "stone", "coal", "stone", "stone", "stone", "stone",
            ],
        );
        assert!(r.find(Station::Workbench, &ring, 3, 3).is_some());
        // Recette de la main utilisable à l'établi, pas l'inverse.
        let sand = g(&i, &["sand", "sand", "sand", "sand"]);
        assert!(r.find(Station::Hand, &sand, 2, 2).is_some());
        assert!(r.find(Station::Forge, &sand[..2], 2, 1).is_none());
        let fur = g(&i, &["iron_ore", "coal"]);
        assert!(r.find(Station::Hand, &fur, 2, 1).is_none());
    }

    #[test]
    fn empty_grid_and_extra_items_match_nothing() {
        let (i, r) = setup();
        assert!(r
            .find(Station::Hand, &g(&i, &["", "", "", ""]), 2, 2)
            .is_none());
        let extra = g(&i, &["planks_oak", "planks_oak", "dirt", ""]);
        assert!(r.find(Station::Hand, &extra, 2, 2).is_none());
    }

    #[test]
    fn rejects_bad_json() {
        let (i, _) = setup();
        for bad in [
            r#"{"recipes":[{"station":"hand","result":{"item":"nope"},"shapeless":["stone"]}]}"#,
            r#"{"recipes":[{"station":"hand","result":{"item":"stone"},"shapeless":["nope"]}]}"#,
            r##"{"recipes":[{"station":"hand","result":{"item":"stone"},"shapeless":["#nope"]}]}"##,
            r#"{"recipes":[{"station":"hand","result":{"item":"stone"},"pattern":["SSS"],"key":{"S":"dirt"}}]}"#,
            r#"{"recipes":[{"station":"hand","result":{"item":"stone"},"pattern":["S","SS"],"key":{"S":"dirt"}}]}"#,
            r#"{"recipes":[{"station":"hand","result":{"item":"stone"},"pattern":["S"],"key":{}}]}"#,
            r#"{"recipes":[{"station":"hand","result":{"item":"stone","count":0},"shapeless":["dirt"]}]}"#,
        ] {
            assert!(RecipeBook::from_json(&i, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_recipe_output_is_reachable_by_its_own_pattern() {
        // Chaque recette doit être retrouvée par `find` avec ses propres ingrédients (aucune
        // recette masquée par une recette précédente).
        let (i, r) = setup();
        for rec in r.recipes() {
            let (w, h) = rec.station.grid_size();
            let mut grid = vec![None; w * h];
            match &rec.shape {
                Shape::Shaped {
                    width,
                    height,
                    cells,
                } => {
                    for y in 0..*height {
                        for x in 0..*width {
                            grid[y * w + x] =
                                cells[y * width + x].as_ref().map(Ingredient::first_item);
                        }
                    }
                }
                Shape::Shapeless(v) => {
                    for (k, ing) in v.iter().enumerate() {
                        grid[k] = Some(ing.first_item());
                    }
                }
            }
            let found = r
                .find(rec.station, &grid, w, h)
                .unwrap_or_else(|| panic!("{} introuvable", i.name_of(rec.output.item)));
            assert_eq!(found.output, rec.output, "{}", i.name_of(rec.output.item));
        }
    }
}
