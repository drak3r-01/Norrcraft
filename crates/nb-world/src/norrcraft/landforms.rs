//! Formes de relief Norrcraft posées sur la fonction de hauteur (session 1.3 ; GDD doc 02 §5.5 à
//! §5.8).
//!
//! Elles s'appliquent, dans l'ordre du doc, au relief `h₄` de [`super::height`] :
//!
//! 1. **archipel** (skärgård, §5.5) : îlots de granit sur le plateau continental baltique ;
//! 2. **côtes** (§5.6) : klint calcaire, côte soulevée en terrasses, dunes, pré salé (les fjords
//!    sont déjà creusés en §5.4) ;
//! 3. **reliefs glaciaires de plaine** (§5.7) : eskers, marmites, drumlins ;
//! 4. **tourbières, palses, champ de lave, glacier** (§5.8).
//!
//! Le glacier vient en dernier : il ne change pas le sol (`h`) mais pose une épaisseur de glace
//! au-dessus ([`Forms::ice`]). Tout est continu, sauf les falaises déclarées par
//! [`Forms::cliff`] (klint, crevasses, fissures volcaniques), seules marches permises par le test
//! de continuité (doc 02 §13). L'eau douce (gouilles, marmites, lac proglaciaire) est laissée à
//! l'hydrographie (session 1.4) ; la mer remplit déjà tout ce qui est sous S.

use super::climate::altitudes;
use super::config::NorrConfig;
use super::fields::{MacroFields, MacroSample, Province};
use super::height::soft_floor;
use super::math::{bump, clamp01, lerp, smooth, spread, GradField, Worley};

/// Constantes de semence des formes (`graine XOR constante`). Figées : en changer une change tous
/// les mondes (et les empreintes de `tests/norrcraft_relief.rs`).
mod seeds {
    pub const ARCHIPELAGO: u64 = 0x4E52_5743_0000_0301;
    pub const ISLANDS: u64 = 0x4E52_5743_0000_0302;
    pub const DUNES: u64 = 0x4E52_5743_0000_0303;
    pub const ESKER: u64 = 0x4E52_5743_0000_0304;
    pub const ESKER_HEIGHT: u64 = 0x4E52_5743_0000_0305;
    pub const KETTLES: u64 = 0x4E52_5743_0000_0306;
    pub const DRUMLIN_FIELD: u64 = 0x4E52_5743_0000_0307;
    pub const DRUMLINS: u64 = 0x4E52_5743_0000_0308;
    pub const MIRE: u64 = 0x4E52_5743_0000_0309;
    pub const PALSES: u64 = 0x4E52_5743_0000_030A;
    pub const LAVA: u64 = 0x4E52_5743_0000_030B;
    pub const FISSURES: u64 = 0x4E52_5743_0000_030C;
    pub const CRATERS: u64 = 0x4E52_5743_0000_030D;
    pub const SHIELDS: u64 = 0x4E52_5743_0000_030E;
    pub const CREVASSES: u64 = 0x4E52_5743_0000_030F;
}

/// Gain sur la densité d'archipel K (puis bornée à 1). Avec la formule nue du doc, K dépasse
/// rarement 0,6 (produit de quatre facteurs dont `1 − 0,7 · O` ≈ 0,8 côté baltique) et les
/// îlots n'émergeaient presque jamais : 0,1 % de la carte.
pub const ARCHIPELAGO_BOOST: f64 = 1.6;
/// Pas de la grille des îlots (doc : Worley de pas 44).
pub const ISLAND_PITCH: f64 = 44.0;
/// Hauteur d'un îlot bornée à `0,9 · rᵢ` (le doc donne `aᵢ` indépendant de `rᵢ` : un écueil de
/// 6 blocs de rayon pouvait monter de 18 blocs, une aiguille).
pub const ISLAND_MAX_HEIGHT_PER_RADIUS: f64 = 0.9;
/// Demi-largeur des eskers, en unités de `Z(p/1300)` (doc : 0,012).
pub const ESKER_WIDTH: f64 = 0.012;
/// Demi-largeur des fissures du champ de lave, en unités de `Z(p/700)` (doc : 0,01).
pub const FISSURE_WIDTH: f64 = 0.01;

/// Type de côte dominant d'une colonne (doc 02 §5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Coast {
    /// Pas une côte particulière (intérieur des terres, haute mer, côte rocheuse ordinaire).
    #[default]
    None,
    /// Archipel : `K > 0,3`.
    Skargard,
    /// Klint calcaire : marche franche de 12 blocs.
    Klint,
    /// Côte soulevée de granit rouge : terrasses de 4 blocs, champs de galets.
    Raised,
    /// Rivage de sable et dunes.
    Dunes,
    /// Pré salé (strandäng) : bande basse et plate.
    SaltMeadow,
}

/// Ce que les formes de relief ont fait à une colonne : utile aux biomes (2.1), à l'hydrographie
/// (1.4), aux décors et au remplissage des blocs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Forms {
    /// Relief filtré `Hl` : la hauteur sans les collines ni le détail (§5.2). Sert au niveau
    /// d'eau en escalier W des tourbières et à la surface des calottes glaciaires.
    pub filtered: f64,
    /// Densité d'archipel K (§5.5), dans [0, 1].
    pub archipelago: f64,
    /// Type de côte dominant (§5.6).
    pub coast: Coast,
    /// Hauteur ajoutée par un esker (blocs).
    pub esker: f64,
    /// Hauteur ajoutée par un drumlin (blocs).
    pub drumlin: f64,
    /// Profondeur d'une marmite (blocs).
    pub kettle: f64,
    /// Masque de tourbière My (§5.8), dans [0, 1].
    pub mire: f64,
    /// Cordon de tourbe (strängar) d'une tourbière ; sinon gouille.
    pub mire_ridge: bool,
    /// Hauteur d'une palse (blocs) : son cœur est du permafrost.
    pub palsa: f64,
    /// Poids du champ de lave `sm(0 ; 0,5 ; Gt)`, dans [0, 1] (1 dès que Gt > 0,5).
    pub lava: f64,
    /// Hauteur du bourrelet de moraine glaciaire (blocs).
    pub moraine: f64,
    /// Épaisseur de glace au-dessus du sol (blocs) : calotte, langue glaciaire.
    pub ice: f64,
    /// Falaise déclarée (klint, crevasse, fissure) : les marches de plus de 3 blocs y sont permises.
    pub cliff: bool,
}

/// Ce que la fonction de hauteur transmet aux formes, pour une colonne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseRelief {
    /// `h₄` (après les vallées).
    pub h: f64,
    /// Relief filtré `Hl`.
    pub filtered: f64,
    /// Poids de terre `sm(0,45 ; 0,52 ; C)`.
    pub land: f64,
    /// `g · u(t)` de la vallée principale.
    pub valley: f64,
    /// Altitude des épaulements, avant creusement des vallées.
    pub before_valleys: f64,
}

/// Partie fractionnaire, dans [0, 1).
#[inline]
fn fract(v: f64) -> f64 {
    v - v.floor()
}

/// Bruits et grilles des formes d'un monde. Cloner est peu coûteux.
#[derive(Debug, Clone)]
pub struct Landforms {
    archipelago: GradField,
    islands: Worley,
    dunes: GradField,
    esker: GradField,
    esker_height: GradField,
    kettles: Worley,
    drumlin_field: GradField,
    drumlins: Worley,
    mire: GradField,
    palses: Worley,
    lava: GradField,
    fissures: GradField,
    craters: Worley,
    shields: Worley,
    crevasses: GradField,
}

impl Landforms {
    pub fn new(seed: i64) -> Self {
        Self {
            archipelago: GradField::new(seed, seeds::ARCHIPELAGO),
            islands: Worley::new(seed, seeds::ISLANDS, ISLAND_PITCH),
            dunes: GradField::new(seed, seeds::DUNES),
            esker: GradField::new(seed, seeds::ESKER),
            esker_height: GradField::new(seed, seeds::ESKER_HEIGHT),
            kettles: Worley::new(seed, seeds::KETTLES, 64.0),
            drumlin_field: GradField::new(seed, seeds::DRUMLIN_FIELD),
            drumlins: Worley::new(seed, seeds::DRUMLINS, 90.0),
            mire: GradField::new(seed, seeds::MIRE),
            palses: Worley::new(seed, seeds::PALSES, 26.0),
            lava: GradField::new(seed, seeds::LAVA),
            fissures: GradField::new(seed, seeds::FISSURES),
            craters: Worley::new(seed, seeds::CRATERS, 70.0),
            shields: Worley::new(seed, seeds::SHIELDS, 2400.0),
            crevasses: GradField::new(seed, seeds::CREVASSES),
        }
    }

    /// Applique les formes à la colonne `(x, z)` : renvoie le sol final et la décomposition.
    pub fn apply(
        &self,
        x: f64,
        z: f64,
        m: &MacroSample,
        fields: &MacroFields,
        cfg: &NorrConfig,
        base: BaseRelief,
    ) -> (f64, Forms) {
        let s = f64::from(cfg.sea_level);
        let (c, u, e, o, t0) = (m.c, m.u, m.e, m.o, m.t0);
        let th = m.theta;
        let mut h = base.h;
        let mut f = Forms {
            filtered: base.filtered,
            ..Forms::default()
        };

        // §5.5 Archipel.
        let gain = cfg.preset.archipelago_gain();
        let k_macro = ARCHIPELAGO_BOOST * (1.0 - u) * bump(c, 0.43, 0.07) * (1.0 - 0.7 * o) * gain;
        if k_macro > 0.0 {
            // Le gain du préréglage (Skärgård : × 2) élargit aussi le masque : sans cela K
            // saturait à 1 sur la même surface et l'archipel ne s'étendait pas.
            let widen = 0.2 * (gain - 1.0);
            let mask = smooth(
                0.50 - widen,
                0.65 - widen,
                spread(self.archipelago.fbm(x / 1800.0, z / 1800.0, 2)),
            );
            let k = clamp01(k_macro * mask);
            if k > 0.0 {
                let top = self.island_height(x, z, th);
                h = lerp(h, s - 7.0 + 2.0 * top, k);
                f.archipelago = k;
                if k > 0.3 {
                    f.coast = Coast::Skargard;
                }
            }
        }
        let k = f.archipelago;

        // §5.6 Côtes (le fjord est déjà creusé par §5.4). Bande côtière : C < 0,50, éteinte à 0,56 ;
        // le klint s'étend un peu plus loin dans les terres (plateau calcaire).
        if (0.38..0.62).contains(&c) {
            let shore = smooth(0.56, 0.50, c);
            let (province, pw) = fields.province_blend(x, z);
            let not_isles = 1.0 - smooth(0.2, 0.4, k);

            let mut klint = 0.0;
            if province == Province::Limestone {
                klint = pw * smooth(0.47, 0.42, e) * smooth(0.62, 0.55, c) * not_isles;
                if klint > 0.0 {
                    let step = smooth(0.468, 0.474, c);
                    h += 12.0 * klint * step;
                    if klint > 0.05 && step > 0.01 && step < 0.99 {
                        f.cliff = true;
                    }
                    if klint > 0.5 {
                        f.coast = Coast::Klint;
                    }
                }
            }

            let mut raised = 0.0;
            if province == Province::GraniteRed {
                raised = pw
                    * smooth(0.55, 0.45, o)
                    * smooth(0.22, 0.27, t0)
                    * smooth(0.53, 0.48, t0)
                    * shore
                    * not_isles;
                let band = smooth(s - 1.0, s + 1.0, h) * smooth(s + 26.0, s + 22.0, h);
                let w = 0.7 * raised * band;
                if w > 0.0 {
                    // `4 · arrondi(h / 4)` adouci : la marche monte sur les 0,4 dernier de l'intervalle.
                    let level = 4.0 * soft_floor(h / 4.0 + 0.5, 0.4);
                    h -= w * (h - level);
                    if raised * band > 0.5 {
                        f.coast = Coast::Raised;
                    }
                }
            }

            let ordinary = (1.0 - klint) * (1.0 - raised) * not_isles * (1.0 - u);
            if ordinary > 0.0 && shore > 0.0 {
                // Pré salé : la bande basse est tirée entre S + 1 et S + 2.
                let meadow = smooth(0.72, 0.78, e) * smooth(0.48, 0.52, t0) * shore * ordinary;
                if meadow > 0.0 {
                    let band = smooth(s - 3.0, s - 1.0, h) * smooth(s + 8.0, s + 5.0, h);
                    h = lerp(h, h.clamp(s + 1.0, s + 2.0), meadow * band);
                    if meadow * band > 0.5 {
                        f.coast = Coast::SaltMeadow;
                    }
                }
                // Sable et dunes : `+3 · Rg(p/40; 2)` sur la bande littorale (≈ 30 blocs).
                let dunes = smooth(0.50, 0.60, e) * shore * ordinary * (1.0 - meadow);
                if dunes > 0.0 {
                    let band = bump(h, s + 2.5, 4.0);
                    if band > 0.0 {
                        h += 3.0 * self.dunes.ridged(x / 40.0, z / 40.0, 2) * dunes * band;
                    }
                    if dunes > 0.5 && h < s + 6.0 && h > s - 4.0 && f.coast == Coast::None {
                        f.coast = Coast::Dunes;
                    }
                }
            }
        }

        // §5.7 Eskers et marmites : zone boréale basse (C > 0,55, U < 0,2).
        let esker_zone = smooth(0.55, 0.60, c) * smooth(0.22, 0.15, u) * base.land;
        if esker_zone > 0.0 {
            let t = self.esker.zero_line(x / 1300.0, z / 1300.0, 2) / ESKER_WIDTH;
            if t < 1.0 {
                let height = 5.0 + 3.0 * self.esker_height.n(x / 300.0, z / 300.0);
                let add = esker_zone * height * (1.0 - t * t);
                h += add;
                f.esker = add;
            }
            let kettle = esker_zone * self.kettle_depth(x, z);
            if kettle > 0.0 {
                h -= kettle;
                f.kettle = kettle;
            }
        }

        // Drumlins : champs de bosses allongées le long de θ, en plaine (U bas, E haut).
        let drumlin_zone = base.land * smooth(0.22, 0.12, u) * smooth(0.45, 0.60, e);
        if drumlin_zone > 0.0 {
            let field = smooth(
                0.55,
                0.70,
                spread(self.drumlin_field.fbm(x / 1500.0, z / 1500.0, 2)),
            );
            let w = drumlin_zone * field;
            if w > 0.0 {
                let add = w * self.drumlin_height(x, z, th);
                h += add;
                f.drumlin = add;
            }
        }

        // §5.8 Tourbières (zones boréale et subarctique), palses (subarctique).
        let mire_zone = smooth(0.08, 0.14, t0) * smooth(0.56, 0.48, t0);
        let my =
            smooth(0.62, 0.75, m.hm) * smooth(0.60, 0.78, e) * base.land * (1.0 - u) * mire_zone;
        if my > 0.0 {
            // Niveau d'eau en escalier W (§6.1, `hydro::stair_level`), marches adoucies comme les
            // fonds de vallée : égal à W sur 70 % de chaque marche, continu sur le reste.
            let w = s + 6.0 * soft_floor((base.filtered - s - 2.0).max(0.0) / 6.0, 0.3);
            let r =
                fract((x * th[0] + z * th[1]) / 9.0 + 0.8 * self.mire.fbm(x / 30.0, z / 30.0, 2));
            let ridge = r < 0.35;
            // Cordon : sol jusqu'à W (un bloc au-dessus de l'eau) ; gouille : fond en W − 1, que
            // l'hydrographie remplit jusqu'à W (eau affleurante).
            h = lerp(h, w + if ridge { 1.0 } else { -0.5 }, my);
            f.mire = my;
            f.mire_ridge = ridge;
            let palsa_zone = my * smooth(0.30, 0.24, t0);
            if palsa_zone > 0.0 {
                let add = palsa_zone * self.palsa_height(x, z);
                h += add;
                f.palsa = add;
            }
        }

        // §5.8 Champ de lave : plaine de basalte bosselée, volcan-bouclier, pseudo-cratères,
        // fissures.
        if m.gt > 0.0 {
            let fissure = self.fissures.zero_line(x / 700.0, z / 700.0, 2) / FISSURE_WIDTH;
            let mut target = s
                + 6.0
                + 5.0 * self.lava.ridged(x / 90.0, z / 90.0, 3)
                + self.shield_height(x, z)
                + self.crater_height(x, z);
            if fissure < 1.0 {
                target -= 8.0 * (1.0 - fissure * fissure);
                if m.gt > 0.01 && fissure < 0.95 {
                    f.cliff = true;
                }
            }
            // Le doc tire h vers la plaine avec le poids Gt ; un poids qui sature dès Gt = 0,5 fait
            // émerger toute la province (biome Lavafält si Gt > 0,5, doc §7.2), même au-dessus des
            // grands fonds, la pente Gt de 0 à 0,5 formant le flanc de l'île.
            let w = smooth(0.0, 0.5, m.gt);
            h = lerp(h, target, w);
            f.lava = w;
        }

        // §5.8 Glacier : au-dessus de la ligne des neiges, dans la chaîne.
        let chain = smooth(0.25, 0.45, u);
        if chain > 0.0 {
            let alt = altitudes(t0, cfg.sea_level);
            let (y_a, y_n) = (alt.tree_line, alt.snow_line);
            // Langues : la glace descend dans les auges (§5.4) dont les épaulements dépassent y_n,
            // jusqu'à la limite des arbres y_a.
            let shoulder = chain * base.valley * smooth(y_n - 8.0, y_n, base.before_valleys);
            // Moraines : bourrelet de gravier sous le contour de la calotte et au front des langues.
            let moraine = (3.0 * chain * bump(base.filtered, y_n - 5.0, 3.0))
                .max(3.0 * shoulder * bump(h, y_a - 2.0, 3.0));
            if moraine > 0.0 {
                h += moraine;
                f.moraine = moraine;
            }
            // Calotte : surface G = Hl + 5, la glace remplit le volume entre h et G.
            let cap = chain * smooth(y_n - 2.0, y_n + 4.0, base.filtered);
            let tongue = shoulder * smooth(y_a, y_a + 10.0, h);
            let mut ice = ((base.filtered + 5.0 - h).max(0.0) * cap).max(8.0 * tongue);
            // Crevasses : bandes transverses au sens d'écoulement, dans les langues épaisses.
            if tongue > 0.3 && ice > 6.0 {
                let r = fract(
                    (x * th[0] + z * th[1]) / 14.0
                        + 0.3 * self.crevasses.fbm(x / 60.0, z / 60.0, 2),
                );
                if r < 0.08 {
                    let depth = 6.0 + 14.0 * self.crevasses.n(x / 40.0 + 77.7, z / 40.0 + 33.3);
                    ice -= depth.min(ice - 1.0);
                    f.cliff = true;
                }
            }
            f.ice = ice;
        }

        (h, f)
    }

    /// Hauteur `maxᵢ bᵢ` des îlots de la grille de pas 44 (§5.5) : rayon `6 + 20 · hᵢ³`, hauteur
    /// `1 + 8 · hᵢ'²` bornée à `0,9 · rᵢ`, allongés le long de θ (1,8:1), plus abrupts côté aval.
    fn island_height(&self, x: f64, z: f64, th: [f64; 2]) -> f64 {
        let p = self.islands.pitch();
        let (ci, cj) = ((x / p).floor() as i32, (z / p).floor() as i32);
        let mut best = 0.0f64;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let c = self.islands.point(i, j);
                let (dx, dz) = (x - c[0], z - c[1]);
                let along = dx * th[0] + dz * th[1];
                let across = dz * th[0] - dx * th[1];
                let hr = self.islands.cell_rand(i, j, 0);
                let r = 6.0 + 20.0 * hr * hr * hr;
                let ha = self.islands.cell_rand(i, j, 1);
                let a = (1.0 + 8.0 * ha * ha).min(ISLAND_MAX_HEIGHT_PER_RADIUS * r);
                let d2 = (along / 1.8) * (along / 1.8) + across * across;
                let q = 1.0 - d2 / (r * r);
                if q > 0.0 {
                    best = best.max(a * q * q * (1.0 + 0.5 * along.max(0.0) / r));
                }
            }
        }
        best
    }

    /// Profondeur des marmites (dödisgropar) : creux circulaires de 3 à 5 blocs, rayon 5 à 9, au
    /// pied des eskers (le centre doit être à 1,2 à 3,2 demi-largeurs de l'axe). Une cellule sur
    /// deux de la grille de pas 64 en porte une ; seuls les deux points les plus proches comptent.
    fn kettle_depth(&self, x: f64, z: f64) -> f64 {
        let hit = self.kettles.eval(x, z);
        let mut best = 0.0f64;
        for (cell, center, d) in [
            (hit.cell, hit.center, hit.f1),
            (hit.cell2, hit.center2, hit.f2),
        ] {
            if self.kettles.cell_rand(cell.0, cell.1, 0) >= 0.5 {
                continue;
            }
            let r = 5.0 + 4.0 * self.kettles.cell_rand(cell.0, cell.1, 1);
            if d >= r {
                continue;
            }
            let near = bump(
                self.esker
                    .zero_line(center[0] / 1300.0, center[1] / 1300.0, 2)
                    / ESKER_WIDTH,
                2.2,
                1.0,
            );
            if near > 0.0 {
                let depth = 3.0 + 2.0 * self.kettles.cell_rand(cell.0, cell.1, 2);
                let q = 1.0 - (d / r) * (d / r);
                best = best.max(depth * near * q * q);
            }
        }
        best
    }

    /// Drumlins : grille de pas 90, 7 cellules sur 10, rayon 14 à 22, hauteur 3 à 5, allongés
    /// le long de θ (3:1).
    fn drumlin_height(&self, x: f64, z: f64, th: [f64; 2]) -> f64 {
        let p = self.drumlins.pitch();
        let (ci, cj) = ((x / p).floor() as i32, (z / p).floor() as i32);
        let mut best = 0.0f64;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                if self.drumlins.cell_rand(i, j, 0) >= 0.7 {
                    continue;
                }
                let c = self.drumlins.point(i, j);
                let (dx, dz) = (x - c[0], z - c[1]);
                let along = (dx * th[0] + dz * th[1]) / 3.0;
                let across = dz * th[0] - dx * th[1];
                let r = 14.0 + 8.0 * self.drumlins.cell_rand(i, j, 1);
                let q = 1.0 - (along * along + across * across) / (r * r);
                if q > 0.0 {
                    let a = 3.0 + 2.0 * self.drumlins.cell_rand(i, j, 2);
                    best = best.max(a * q * q);
                }
            }
        }
        best
    }

    /// Palses : grille de pas 26, 55 % des cellules, buttes de 3 à 6 blocs, rayon 5 à 9
    /// (profil parabolique : pente bornée à `2a / r`).
    fn palsa_height(&self, x: f64, z: f64) -> f64 {
        let p = self.palses.pitch();
        let (ci, cj) = ((x / p).floor() as i32, (z / p).floor() as i32);
        let mut best = 0.0f64;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                if self.palses.cell_rand(i, j, 0) >= 0.55 {
                    continue;
                }
                let c = self.palses.point(i, j);
                let (dx, dz) = (x - c[0], z - c[1]);
                let r = 5.0 + 4.0 * self.palses.cell_rand(i, j, 1);
                let q = 1.0 - (dx * dx + dz * dz) / (r * r);
                if q > 0.0 {
                    let a = 3.0 + 3.0 * self.palses.cell_rand(i, j, 2);
                    best = best.max(a * q);
                }
            }
        }
        best
    }

    /// Volcan-bouclier : un par cellule de la grille de pas 2400, +30 au sommet, rayon 450 à 600
    /// (pente douce, moins de 0,15).
    fn shield_height(&self, x: f64, z: f64) -> f64 {
        let p = self.shields.pitch();
        let (ci, cj) = ((x / p).floor() as i32, (z / p).floor() as i32);
        let mut best = 0.0f64;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let c = self.shields.point(i, j);
                let (dx, dz) = (x - c[0], z - c[1]);
                let r = 450.0 + 150.0 * self.shields.cell_rand(i, j, 0);
                best = best.max(30.0 * bump((dx * dx + dz * dz).sqrt(), 0.0, r));
            }
        }
        best
    }

    /// Pseudo-cratères : grille de pas 70, une cellule sur deux, anneau de 4 à 7 blocs de haut
    /// (rayon 10 à 16, crête à 0,45 r, cuvette au centre).
    fn crater_height(&self, x: f64, z: f64) -> f64 {
        let p = self.craters.pitch();
        let (ci, cj) = ((x / p).floor() as i32, (z / p).floor() as i32);
        let mut best = 0.0f64;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                if self.craters.cell_rand(i, j, 0) >= 0.5 {
                    continue;
                }
                let c = self.craters.point(i, j);
                let (dx, dz) = (x - c[0], z - c[1]);
                let d = (dx * dx + dz * dz).sqrt();
                let r = 10.0 + 6.0 * self.craters.cell_rand(i, j, 1);
                if d < r {
                    let a = 4.0 + 3.0 * self.craters.cell_rand(i, j, 2);
                    best = best.max(a * bump(d, 0.45 * r, 0.55 * r));
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fract_is_in_unit_interval() {
        for v in [-3.25, -0.5, 0.0, 0.75, 12.5] {
            let f = fract(v);
            assert!((0.0..1.0).contains(&f), "{v} → {f}");
        }
        assert_eq!(fract(-0.25), 0.75);
    }

    #[test]
    fn bumps_are_bounded() {
        let l = Landforms::new(4242);
        let th = [0.6, 0.8];
        for j in -60..60 {
            for i in -60..60 {
                let (x, z) = (f64::from(i) * 7.3, f64::from(j) * 5.9);
                let isl = l.island_height(x, z, th);
                assert!((0.0..=9.0 * 1.9).contains(&isl), "{isl}");
                assert!((0.0..=5.0).contains(&l.drumlin_height(x, z, th)));
                assert!((0.0..=6.0).contains(&l.palsa_height(x, z)));
                assert!((0.0..=7.0).contains(&l.crater_height(x, z)));
                assert!((0.0..=30.0).contains(&l.shield_height(x * 20.0, z * 20.0)));
                assert!((0.0..=5.0).contains(&l.kettle_depth(x, z)));
            }
        }
    }
}
