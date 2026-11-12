//! Hardes persistantes et migrations (doc 03 §4.4, session 5.3). Modèle pur, sans ECS.
//!
//! Une harde est un objet léger du monde : une par espèce et par cellule (1024 blocs par défaut).
//! Sa **position** est une fonction pure de la graine, de la cellule et de la date : elle suit le
//! calendrier même quand aucun chunk n'est chargé, sans rien simuler. Son **effectif** est le seul
//! état : il baisse quand des individus meurent et remonte de `regrow_per_day` par jour de jeu, et
//! c'est lui que la sauvegarde garde ([`HerdRecord`]).
//!
//! Trois mouvements ([`HerdMotion`]) :
//! * `Seasonal` : transhumance entre un site d'été et un site d'hiver, aux mois de montée et de
//!   descente (rennes en mai et en octobre), le long d'une courbe qui s'écarte de la droite ;
//! * `Territory` : territoire fixe parcouru lentement (meutes de loups) ;
//! * `Pass` : présente seulement pendant une fenêtre du calendrier, elle traverse la cellule de
//!   son site de départ à son site d'arrivée, ou en ligne droite selon un cap (vols de grues et
//!   d'oies, remontée des saumons, bancs de harengs en route).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use nb_data::{HerdMotion, HerdRules};

/// Largeur d'une transhumance, en fraction d'année (≈ 22 jours).
pub const TRANSIT: f64 = 0.06;
/// Côté de la grille de recherche des sites dans une cellule (8 × 8 échantillons).
const SITE_GRID: i32 = 8;

/// État sauvegardé d'une harde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HerdRecord {
    pub species: String,
    pub cx: i32,
    pub cz: i32,
    /// Effectif courant (fractionnaire pendant la repousse).
    pub count: f32,
    /// Dernier instant mis à jour, en années de jeu (année + fraction).
    pub time: f64,
}

/// Une harde vue à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Herd {
    pub cx: i32,
    pub cz: i32,
    /// Position (x, z) en blocs.
    pub pos: (f64, f64),
    /// Individus actuels (arrondi de l'effectif).
    pub count: i32,
    /// Effectif d'une harde pleine.
    pub base: i32,
}

/// Instant et espèce d'une requête.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    pub seed: i64,
    pub species: &'a str,
    pub rules: &'a HerdRules,
    /// Fraction d'année (0 = solstice d'hiver).
    pub year_fraction: f64,
    /// Années de jeu écoulées (année + fraction), pour la repousse.
    pub time: f64,
    pub days_per_year: f64,
}

/// Sites d'une cellule : départ (hiver) et arrivée (été).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sites {
    winter: (f64, f64),
    summer: (f64, f64),
}

type Key = (String, i32, i32);

/// Registre des hardes d'un monde.
#[derive(Debug, Default, Clone)]
pub struct HerdBook {
    records: HashMap<Key, HerdRecord>,
    /// Sites trouvés (`None` : la cellule n'a pas les biomes de l'espèce). Recalculés au besoin.
    sites: HashMap<Key, Option<Sites>>,
}

impl HerdBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recharge les effectifs d'une sauvegarde.
    pub fn from_records(records: Vec<HerdRecord>) -> Self {
        Self {
            records: records
                .into_iter()
                .map(|r| ((r.species.clone(), r.cx, r.cz), r))
                .collect(),
            sites: HashMap::new(),
        }
    }

    /// Effectifs à sauvegarder, dans un ordre stable.
    pub fn records(&self) -> Vec<HerdRecord> {
        let mut v: Vec<HerdRecord> = self.records.values().cloned().collect();
        v.sort_by(|a, b| (&a.species, a.cx, a.cz).cmp(&(&b.species, b.cx, b.cz)));
        v
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Les sites de cette cellule ont-ils déjà été cherchés (coûteux la première fois) ?
    pub fn is_scanned(&self, species: &str, cx: i32, cz: i32) -> bool {
        self.sites.contains_key(&(species.to_owned(), cx, cz))
    }

    /// Effectif enregistré (sans repousse), s'il existe.
    pub fn count(&self, species: &str, cx: i32, cz: i32) -> Option<f32> {
        self.records
            .get(&(species.to_owned(), cx, cz))
            .map(|r| r.count)
    }

    /// Fixe l'effectif (individus réabsorbés, pertes comprises).
    pub fn set_count(&mut self, species: &str, cx: i32, cz: i32, count: f32, time: f64) {
        if let Some(r) = self.records.get_mut(&(species.to_owned(), cx, cz)) {
            r.count = count.max(0.0);
            r.time = time;
        }
    }

    /// La harde de la cellule `(cx, cz)` à l'instant de `q`, ou `None` si la cellule n'a pas les
    /// biomes de l'espèce ou si la harde est absente (hors de sa fenêtre de passage). La repousse
    /// est rattrapée ici, au premier regard après une absence.
    pub fn herd_at(
        &mut self,
        q: &Query<'_>,
        cx: i32,
        cz: i32,
        biome: &dyn Fn(i32, i32) -> Option<String>,
    ) -> Option<Herd> {
        let key: Key = (q.species.to_owned(), cx, cz);
        let sites = *self
            .sites
            .entry(key.clone())
            .or_insert_with(|| find_sites(q, cx, cz, biome));
        let sites = sites?;
        let base = base_size(q, cx, cz);
        let rec = self.records.entry(key).or_insert_with(|| HerdRecord {
            species: q.species.to_owned(),
            cx,
            cz,
            count: base as f32,
            time: q.time,
        });
        let elapsed_days = ((q.time - rec.time) * q.days_per_year).max(0.0) as f32;
        rec.count = (rec.count + q.rules.regrow_per_day * elapsed_days).min(base as f32);
        rec.time = q.time;
        let pos = position(q, cx, cz, &sites)?;
        Some(Herd {
            cx,
            cz,
            pos,
            count: rec.count.round() as i32,
            base,
        })
    }
}

/// Cellule contenant le point `(x, z)`.
pub fn cell_of(rules: &HerdRules, x: f64, z: f64) -> (i32, i32) {
    let c = f64::from(rules.cell);
    ((x / c).floor() as i32, (z / c).floor() as i32)
}

/// Coin minimal de la cellule.
fn origin(rules: &HerdRules, cx: i32, cz: i32) -> (f64, f64) {
    (
        f64::from(cx) * f64::from(rules.cell),
        f64::from(cz) * f64::from(rules.cell),
    )
}

/// Début du mois réel `m` (0 = januari) en fraction d'année depuis le solstice d'hiver (21 déc.).
pub fn month_fraction(m: u8) -> f64 {
    ((f64::from(m) * 30.4 + 11.0) / 365.25).rem_euclid(1.0)
}

#[inline]
fn smooth(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Part d'été (0 = site d'hiver, 1 = site d'été) à la fraction d'année `a` : montée au mois
/// `up_month`, descente au mois `down_month`, chacune sur [`TRANSIT`].
pub fn summer_share(rules: &HerdRules, a: f64) -> f64 {
    let up = month_fraction(rules.up_month);
    let span = (month_fraction(rules.down_month) - up).rem_euclid(1.0);
    let v = (a - up).rem_euclid(1.0);
    if v < span.min(TRANSIT) {
        smooth(v / TRANSIT)
    } else if v < span {
        1.0
    } else if v < span + TRANSIT {
        1.0 - smooth((v - span) / TRANSIT)
    } else {
        0.0
    }
}

/// Progression (0 à 1) dans la fenêtre de passage, `None` hors fenêtre.
pub fn pass_progress(rules: &HerdRules, a: f64) -> Option<f64> {
    let start = month_fraction(rules.pass_months[0]);
    let mut len =
        (month_fraction(rules.pass_months[1].saturating_add(1) % 12) - start).rem_euclid(1.0);
    if len < 1e-6 {
        len = 1.0;
    }
    let v = (a - start).rem_euclid(1.0);
    (v < len).then(|| v / len)
}

/// Hachage SplitMix64 de plusieurs entiers.
fn mix(seed: i64, species: &str, cx: i32, cz: i32, salt: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for b in species.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    let mut z = (seed as u64)
        ^ h
        ^ (cx as u32 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (cz as u32 as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ salt.wrapping_mul(0x1656_67b1_9e37_79f9);
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Réel dans [0, 1) tiré d'un hachage.
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Effectif d'une harde pleine de la cellule.
pub fn base_size(q: &Query<'_>, cx: i32, cz: i32) -> i32 {
    let (lo, hi) = (q.rules.size_min, q.rules.size_max);
    lo + (unit(mix(q.seed, q.species, cx, cz, 1)) * f64::from(hi - lo + 1)) as i32
}

/// Cherche les sites de la cellule : un échantillon de biome tous les `cell / 8` blocs.
fn find_sites(
    q: &Query<'_>,
    cx: i32,
    cz: i32,
    biome: &dyn Fn(i32, i32) -> Option<String>,
) -> Option<Sites> {
    let r = q.rules;
    let (ox, oz) = origin(r, cx, cz);
    let step = f64::from(r.cell) / f64::from(SITE_GRID);
    let mut summer: Vec<(f64, f64)> = Vec::new();
    let mut winter: Vec<(f64, f64)> = Vec::new();
    if !r.summer.is_empty() || !r.winter.is_empty() {
        for i in 0..SITE_GRID {
            for j in 0..SITE_GRID {
                let p = (
                    ox + (f64::from(i) + 0.5) * step,
                    oz + (f64::from(j) + 0.5) * step,
                );
                let Some(b) = biome(p.0 as i32, p.1 as i32) else {
                    continue;
                };
                if r.summer.contains(&b) {
                    summer.push(p);
                }
                if r.winter.contains(&b) {
                    winter.push(p);
                }
            }
        }
    }
    let pick = |v: &[(f64, f64)], salt: u64| -> Option<(f64, f64)> {
        (!v.is_empty()).then(|| v[(mix(q.seed, q.species, cx, cz, salt) % v.len() as u64) as usize])
    };
    let (s, w) = (pick(&summer, 2), pick(&winter, 3));
    match r.motion {
        HerdMotion::Seasonal => {
            let (summer, winter) = match (s, w) {
                (Some(s), Some(w)) => (s, w),
                // Une seule saison possible : harde sédentaire sur ce site.
                (Some(p), None) | (None, Some(p)) => (p, p),
                (None, None) => return None,
            };
            Some(Sites { winter, summer })
        }
        HerdMotion::Territory => {
            let p = s?;
            Some(Sites {
                winter: p,
                summer: p,
            })
        }
        HerdMotion::Pass => match (s, w) {
            (Some(summer), Some(winter)) => Some(Sites { winter, summer }),
            _ => {
                // Ligne droite selon le cap, légèrement décalée de part et d'autre du centre.
                let len = f64::from(r.heading[0]).hypot(f64::from(r.heading[1]));
                if len < 1e-3 {
                    return None;
                }
                let (hx, hz) = (f64::from(r.heading[0]) / len, f64::from(r.heading[1]) / len);
                let c = f64::from(r.cell);
                let shift = (unit(mix(q.seed, q.species, cx, cz, 4)) - 0.5) * 0.5 * c;
                let (mx, mz) = (ox + c * 0.5 - hz * shift, oz + c * 0.5 + hx * shift);
                let reach = c * 0.75;
                Some(Sites {
                    winter: (mx - hx * reach, mz - hz * reach),
                    summer: (mx + hx * reach, mz + hz * reach),
                })
            }
        },
    }
}

/// Position calendaire de la harde, `None` si elle est absente (hors fenêtre de passage).
fn position(q: &Query<'_>, cx: i32, cz: i32, s: &Sites) -> Option<(f64, f64)> {
    let r = q.rules;
    let a = q.year_fraction;
    let (base, bulge) = match r.motion {
        HerdMotion::Seasonal => {
            let t = summer_share(r, a);
            let (dx, dz) = (s.summer.0 - s.winter.0, s.summer.1 - s.winter.1);
            // La route s'écarte de la droite, comme une vallée : ventre maximal à mi-chemin.
            let side = if mix(q.seed, q.species, cx, cz, 5) & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            let off = (t * std::f64::consts::PI).sin() * 0.15 * side;
            (
                (s.winter.0 + dx * t, s.winter.1 + dz * t),
                (-dz * off, dx * off),
            )
        }
        HerdMotion::Territory => (s.summer, (0.0, 0.0)),
        HerdMotion::Pass => {
            let p = pass_progress(r, a)?;
            (
                (
                    s.winter.0 + (s.summer.0 - s.winter.0) * p,
                    s.winter.1 + (s.summer.1 - s.winter.1) * p,
                ),
                (0.0, 0.0),
            )
        }
    };
    // Va-et-vient lent autour de la position calendaire (≈ 40 tours par an, déphasés).
    let phase = unit(mix(q.seed, q.species, cx, cz, 6)) * std::f64::consts::TAU;
    let turn = a * 40.0 * std::f64::consts::TAU + phase;
    let w = f64::from(r.wander);
    Some((
        base.0 + bulge.0 + turn.cos() * w,
        base.1 + bulge.1 + (turn * 0.7).sin() * w,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reindeer() -> HerdRules {
        HerdRules {
            summer: vec!["FJALL".into()],
            winter: vec!["SKOG".into()],
            wander: 0.0,
            ..HerdRules::default()
        }
    }

    /// Cellule (0, 0) : fjäll au nord (z < 512), forêt au sud.
    fn map(x: i32, z: i32) -> Option<String> {
        if !(0..1024).contains(&x) || !(0..1024).contains(&z) {
            return Some("MER".into());
        }
        Some(if z < 512 { "FJALL" } else { "SKOG" }.into())
    }

    fn query<'a>(rules: &'a HerdRules, a: f64) -> Query<'a> {
        Query {
            seed: 7,
            species: "ren",
            rules,
            year_fraction: a,
            time: 3.0 + a,
            days_per_year: 24.0,
        }
    }

    fn herd(book: &mut HerdBook, rules: &HerdRules, a: f64) -> Option<Herd> {
        book.herd_at(&query(rules, a), 0, 0, &map)
    }

    #[test]
    fn reindeer_summer_on_the_fjall_and_winter_in_the_forest() {
        let r = reindeer();
        let mut b = HerdBook::new();
        let summer = herd(&mut b, &r, 0.55).unwrap().pos;
        let winter = herd(&mut b, &r, 0.05).unwrap().pos;
        assert!(summer.1 < 512.0, "été au fjäll : {summer:?}");
        assert!(winter.1 >= 512.0, "hiver en forêt : {winter:?}");
    }

    #[test]
    fn transhumance_happens_in_may_and_october() {
        let r = reindeer();
        let mut b = HerdBook::new();
        let share = |a| summer_share(&r, a);
        // Avril : encore en hiver. Fin mai : arrivée. Septembre : encore en été. Novembre : parti.
        assert_eq!(share(month_fraction(3) + 0.01), 0.0);
        assert!(share(month_fraction(4) + 0.03) > 0.1);
        assert_eq!(share(month_fraction(5) + 0.05), 1.0);
        assert_eq!(share(month_fraction(8) + 0.02), 1.0);
        assert!(share(month_fraction(9) + 0.03) < 0.9);
        assert_eq!(share(month_fraction(10) + 0.05), 0.0);
        // En route (mi-mai) la harde est entre les deux sites.
        let mid = herd(&mut b, &r, month_fraction(4) + TRANSIT / 2.0)
            .unwrap()
            .pos;
        let (w, s) = (
            herd(&mut b, &r, 0.05).unwrap().pos,
            herd(&mut b, &r, 0.6).unwrap().pos,
        );
        assert!(mid.1 > s.1.min(w.1) - 200.0 && mid.1 < s.1.max(w.1) + 200.0);
        assert_ne!(mid, w);
        assert_ne!(mid, s);
    }

    #[test]
    fn position_is_a_pure_function_of_date_and_seed() {
        let r = HerdRules {
            wander: 30.0,
            ..reindeer()
        };
        let a = herd(&mut HerdBook::new(), &r, 0.43);
        let b = herd(&mut HerdBook::new(), &r, 0.43);
        assert_eq!(a, b);
        let other = Query {
            seed: 8,
            ..query(&r, 0.43)
        };
        let c = HerdBook::new().herd_at(&other, 0, 0, &map).unwrap();
        assert_ne!(a.unwrap().pos, c.pos);
        // Le va-et-vient reste dans son rayon.
        let still = herd(&mut HerdBook::new(), &reindeer(), 0.43).unwrap().pos;
        let d =
            ((a.unwrap().pos.0 - still.0).powi(2) + (a.unwrap().pos.1 - still.1).powi(2)).sqrt();
        assert!(d <= 30.0 * 1.5);
    }

    #[test]
    fn a_cell_without_the_biomes_has_no_herd() {
        let r = reindeer();
        let mut b = HerdBook::new();
        let sea = |_: i32, _: i32| Some("MER".to_string());
        assert!(b.herd_at(&query(&r, 0.3), 0, 0, &sea).is_none());
        // Un seul biome disponible : harde sédentaire sur ce site.
        let only = |_: i32, _: i32| Some("SKOG".to_string());
        let h1 = b.herd_at(&query(&r, 0.05), 1, 1, &only).unwrap().pos;
        let h2 = b.herd_at(&query(&r, 0.55), 1, 1, &only).unwrap().pos;
        assert_eq!(h1, h2);
    }

    #[test]
    fn losses_are_kept_and_the_herd_regrows() {
        let r = HerdRules {
            size_min: 10,
            size_max: 10,
            regrow_per_day: 0.5,
            ..reindeer()
        };
        let mut b = HerdBook::new();
        let h = herd(&mut b, &r, 0.55).unwrap();
        assert_eq!((h.count, h.base), (10, 10));
        // Le joueur en tue quatre : six rentrent dans le registre.
        let t = query(&r, 0.55).time;
        b.set_count("ren", 0, 0, 6.0, t);
        assert_eq!(herd(&mut b, &r, 0.55).unwrap().count, 6);
        // 4 jours plus tard (24 jours par an) : +2 individus.
        let later = Query {
            time: t + 4.0 / 24.0,
            ..query(&r, 0.55)
        };
        assert_eq!(b.herd_at(&later, 0, 0, &map).unwrap().count, 8);
        // Jamais au-delà de l'effectif plein.
        let much = Query {
            time: t + 5.0,
            ..query(&r, 0.55)
        };
        assert_eq!(b.herd_at(&much, 0, 0, &map).unwrap().count, 10);
    }

    #[test]
    fn sizes_stay_in_range_and_vary_per_cell() {
        let r = reindeer();
        let q = query(&r, 0.3);
        let sizes: Vec<i32> = (0..30).map(|i| base_size(&q, i, -i)).collect();
        assert!(sizes.iter().all(|s| (6..=14).contains(s)));
        assert!(sizes.iter().any(|&s| s != sizes[0]));
    }

    #[test]
    fn a_passing_flock_exists_only_in_its_window_and_crosses_the_cell() {
        let r = HerdRules {
            motion: HerdMotion::Pass,
            pass_months: [3, 4],
            heading: [0.0, -1.0],
            wander: 0.0,
            ..HerdRules::default()
        };
        let mut b = HerdBook::new();
        let any = |_: i32, _: i32| Some("KUST".to_string());
        let mut at = |a: f64| b.herd_at(&query(&r, a), 0, 0, &any);
        assert!(at(month_fraction(1)).is_none(), "février : pas de passage");
        assert!(at(month_fraction(7)).is_none(), "août : pas de passage");
        let early = at(month_fraction(3) + 0.005).expect("avril");
        let late = at(month_fraction(5) - 0.005).expect("fin mai");
        // Cap vers le nord (−z) : la harde avance dans ce sens et traverse la cellule.
        assert!(late.pos.1 < early.pos.1 - 600.0, "{early:?} → {late:?}");
        assert!(early.pos.1 > 900.0 && late.pos.1 < 150.0);
    }

    #[test]
    fn a_pass_between_two_biome_sites_goes_from_one_to_the_other() {
        // Saumon : de la mer (départ) à la rivière (arrivée) pendant l'été.
        let r = HerdRules {
            motion: HerdMotion::Pass,
            pass_months: [5, 7],
            summer: vec!["FJALL".into()],
            winter: vec!["SKOG".into()],
            wander: 0.0,
            ..HerdRules::default()
        };
        let mut b = HerdBook::new();
        let start = b
            .herd_at(&query(&r, month_fraction(5) + 0.002), 0, 0, &map)
            .unwrap()
            .pos;
        let end = b
            .herd_at(&query(&r, month_fraction(8) - 0.002), 0, 0, &map)
            .unwrap()
            .pos;
        assert!(start.1 >= 512.0 && end.1 < 512.0, "{start:?} → {end:?}");
    }

    #[test]
    fn the_roster_round_trips_through_json() {
        let r = reindeer();
        let mut b = HerdBook::new();
        herd(&mut b, &r, 0.55);
        b.set_count("ren", 0, 0, 3.0, 3.55);
        let json = serde_json::to_string(&b.records()).unwrap();
        let back = HerdBook::from_records(serde_json::from_str(&json).unwrap());
        assert_eq!(back.count("ren", 0, 0), Some(3.0));
        assert_eq!(back.records(), b.records());
        assert_eq!(cell_of(&r, -1.0, 1025.0), (-1, 1));
    }
}
