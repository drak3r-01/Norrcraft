//! Lumière locale recalculée par le mesher (spec §5) : volume 20×130×20 couvrant le chunk et
//! 2 blocs de bordure, passe colonne puis au plus 14 balayages Gauss-Seidel en place.

use crate::block::BlockTable;
use crate::neighborhood::{BlockCache, Neighborhood};

/// Dimensions du volume (`[lx+2][y+1][lz+2]`).
pub const LIGHT_W: usize = 20;
pub const LIGHT_H: usize = 130;
pub const LIGHT_D: usize = 20;
/// Nombre maximal de balayages de diffusion.
pub const LIGHT_MAX_ITERS: u32 = 14;

/// Carte de lumière (`lightMap`) et d'atténuation (`attenuationMap`) du mesher.
#[derive(Clone)]
pub struct LightVolume {
    light: Vec<i32>,
    atten: Vec<u8>,
}

impl std::fmt::Debug for LightVolume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LightVolume(..)")
    }
}

#[inline]
const fn idx(ax: usize, ay: usize, az: usize) -> usize {
    (ax * LIGHT_H + ay) * LIGHT_D + az
}

/// Diffusion Gauss-Seidel en place, ordre x → y → z, au plus [`LIGHT_MAX_ITERS`] balayages :
/// transcription directe du Java (§5.3). Utilisée telle quelle si [`diffuse_active`] refuse.
fn diffuse_reference(light: &mut [i32], atten: &[u8]) {
    let mut changed = true;
    let mut iters = LIGHT_MAX_ITERS;
    while changed && iters > 0 {
        iters -= 1;
        changed = false;
        for x in 0..LIGHT_W {
            for y in 0..LIGHT_H {
                for z in 0..LIGHT_D {
                    let i = idx(x, y, z);
                    let a = i32::from(atten[i] as i8);
                    if a >= 15 {
                        continue;
                    }
                    let prev = light[i];
                    let mut m = prev;
                    if x > 0 {
                        m = m.max(light[idx(x - 1, y, z)] - a);
                    }
                    if x < LIGHT_W - 1 {
                        m = m.max(light[idx(x + 1, y, z)] - a);
                    }
                    if y > 0 {
                        m = m.max(light[idx(x, y - 1, z)] - a);
                    }
                    if y < LIGHT_H - 1 {
                        let up = light[idx(x, y + 1, z)];
                        if up == 15 && a == 1 {
                            m = m.max(15);
                        } else {
                            m = m.max(up - a);
                        }
                    }
                    if z > 0 {
                        m = m.max(light[idx(x, y, z - 1)] - a);
                    }
                    if z < LIGHT_D - 1 {
                        m = m.max(light[idx(x, y, z + 1)] - a);
                    }
                    if m > prev {
                        light[i] = m;
                        changed = true;
                    }
                }
            }
        }
    }
}

// Volume de travail bordé d'une couche de zéros (indices décalés de 1 sur chaque axe).
const PH: usize = LIGHT_H + 2;
const PD: usize = LIGHT_D + 2;
const PAD_VOLUME: usize = (LIGHT_W + 2) * PH * PD;
const STEP_X: usize = PH * PD;
const STEP_Y: usize = PD;

#[inline]
const fn pidx(ax: usize, ay: usize, az: usize) -> usize {
    ((ax + 1) * PH + (ay + 1)) * PD + (az + 1)
}

/// Même calcul que [`diffuse_reference`], résultat identique, mais limité aux cellules qui
/// peuvent encore changer. Renvoie `false` (sans rien modifier) hors de son domaine de validité.
///
/// Équivalence, cellule par cellule et balayage par balayage :
/// * la bordure de zéros remplace les tests de bornes : un voisin absent contribuait
///   `max(m, rien)`, un voisin à 0 contribue `0 − a < 0 ≤ prev` (et jamais la règle du ciel,
///   qui exige `up == 15`) ;
/// * avec toutes les atténuations diffusantes ≥ 1, une nouvelle valeur vaut soit 15, soit un
///   voisin moins au moins 1 : aucune ne dépasse `M = max(15, max initial)`, et une cellule à `M`
///   ne change plus. L'ignorer ne modifie ni sa valeur, ni celles que ses voisins lisent, ni le
///   drapeau `changed` ; l'ordre de visite des autres cellules (x → y → z) est conservé.
///
/// Domaine : atténuations relues (octet signé) ≥ 1 pour les cellules diffusantes et valeurs
/// ≤ 127. C'est toujours le cas avec `blocks.json` ; une table exotique (atténuation > 127
/// relue négative) retombe sur la transcription directe.
fn diffuse_active(light: &mut [i32], atten: &[u8]) -> bool {
    let mut max = 15;
    let mut work = vec![0u8; PAD_VOLUME];
    // Cellules diffusantes non saturées, dans l'ordre x → y → z, avec leur atténuation.
    let mut active: Vec<(u32, u8)> = Vec::new();
    for x in 0..LIGHT_W {
        for y in 0..LIGHT_H {
            for z in 0..LIGHT_D {
                let i = idx(x, y, z);
                let l = light[i];
                if !(0..=127).contains(&l) {
                    return false;
                }
                max = max.max(l);
                work[pidx(x, y, z)] = l as u8;
            }
        }
    }
    for x in 0..LIGHT_W {
        for y in 0..LIGHT_H {
            for z in 0..LIGHT_D {
                let i = idx(x, y, z);
                let a = i32::from(atten[i] as i8);
                if a >= 15 {
                    continue;
                }
                if a < 1 {
                    return false;
                }
                if light[i] < max {
                    active.push((pidx(x, y, z) as u32, a as u8));
                }
            }
        }
    }

    let mut changed = true;
    let mut iters = LIGHT_MAX_ITERS;
    while changed && iters > 0 {
        iters -= 1;
        changed = false;
        let mut kept = 0;
        for r in 0..active.len() {
            let (pi, a) = active[r];
            let (i, a) = (pi as usize, i32::from(a));
            let l = |j: usize| i32::from(work[j]);
            let prev = l(i);
            let up = l(i + STEP_Y);
            let m = prev
                .max(l(i - STEP_X) - a)
                .max(l(i + STEP_X) - a)
                .max(l(i - STEP_Y) - a)
                .max(if up == 15 && a == 1 { 15 } else { up - a })
                .max(l(i - 1) - a)
                .max(l(i + 1) - a);
            if m > prev {
                work[i] = m as u8;
                changed = true;
            }
            if m < max {
                active[kept] = active[r];
                kept += 1;
            }
        }
        active.truncate(kept);
    }

    for x in 0..LIGHT_W {
        for y in 0..LIGHT_H {
            for z in 0..LIGHT_D {
                light[idx(x, y, z)] = i32::from(work[pidx(x, y, z)]);
            }
        }
    }
    true
}

impl LightVolume {
    /// Calcule la lumière du voisinage (§5.2 puis §5.3, ordre exact).
    pub fn compute(n: &Neighborhood, blocks: &BlockTable) -> Self {
        Self::compute_cached(&BlockCache::new(*n, blocks))
    }

    pub(crate) fn compute_cached(cache: &BlockCache) -> Self {
        let (mut light, atten) = column_pass(cache);
        if !diffuse_active(&mut light, &atten) {
            diffuse_reference(&mut light, &atten);
        }
        Self { light, atten }
    }

    /// Volume de référence : passe colonne puis diffusion transcrite telle quelle du Java.
    #[cfg(test)]
    fn compute_reference(cache: &BlockCache) -> Self {
        let (mut light, atten) = column_pass(cache);
        diffuse_reference(&mut light, &atten);
        Self { light, atten }
    }
}

/// Passe colonne (§5.2) : lumière du ciel descendante et carte d'atténuation.
fn column_pass(cache: &BlockCache) -> (Vec<i32>, Vec<u8>) {
    {
        let table = cache.table();
        let mut light = vec![0i32; LIGHT_W * LIGHT_H * LIGHT_D];
        let mut atten = vec![0u8; LIGHT_W * LIGHT_H * LIGHT_D];

        // Passe colonne (ciel descendant).
        for lx in -2..=17i32 {
            for lz in -2..=17i32 {
                let mut cur: i32 = 15;
                for y in (-1..=128i32).rev() {
                    let mut a: i32 = 1;
                    let mut emission: i32 = 0;
                    if (0..128).contains(&y) {
                        let b = table.get(cache.get(lx, y, lz));
                        emission = b.light_emission;
                        a = if b.is_light_blocking() {
                            15
                        } else {
                            b.light_attenuation.max(1)
                        };
                    } else if y < 0 {
                        a = 15;
                    }
                    let i = idx((lx + 2) as usize, (y + 1) as usize, (lz + 2) as usize);
                    // `(byte) atten` : relu comme byte signé lors de la diffusion.
                    atten[i] = a as u8;
                    if y < 128 {
                        if a == 1 && cur == 15 {
                            cur = 15;
                        } else {
                            cur = (cur - a).max(0);
                        }
                        cur = cur.max(emission);
                    }
                    light[i] = cur;
                }
            }
        }
        (light, atten)
    }
}

impl LightVolume {
    /// Valeur brute aux indices du tableau (`lightMap[ax][ay][az]`).
    #[inline]
    pub fn raw(&self, ax: usize, ay: usize, az: usize) -> i32 {
        self.light[idx(ax, ay, az)]
    }

    /// Atténuation brute aux indices du tableau.
    #[inline]
    pub fn attenuation(&self, ax: usize, ay: usize, az: usize) -> u8 {
        self.atten[idx(ax, ay, az)]
    }

    /// Lumière de la cellule locale `(lx, y, lz)` (doit être dans le volume).
    #[inline]
    pub fn at(&self, lx: i32, y: i32, lz: i32) -> i32 {
        self.raw((lx + 2) as usize, (y + 1) as usize, (lz + 2) as usize)
    }

    /// `getL` : lecture clampée aux bornes du volume.
    #[inline]
    pub fn get_clamped(&self, lx: i32, y: i32, lz: i32) -> i32 {
        let ax = (lx + 2).clamp(0, LIGHT_W as i32 - 1) as usize;
        let ay = (y + 1).clamp(0, LIGHT_H as i32 - 1) as usize;
        let az = (lz + 2).clamp(0, LIGHT_D as i32 - 1) as usize;
        self.raw(ax, ay, az)
    }

    /// Empreinte FNV-1a 64 du volume (tests de non-régression).
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &v in &self.light {
            for b in v.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// `getLightScale(l) = max(0.08f, l / 15.0f)`.
#[inline]
pub fn light_scale(l: i32) -> f32 {
    f32::max(0.08, l as f32 / 15.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockMeshInfo, BlockRole};
    use crate::neighborhood::ChunkVoxels;
    use nb_core::math::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
    use nb_data::BlockRegistry;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % u64::from(n)) as u32
        }
    }

    /// Terrain aléatoire : sol plein, grottes, eau, feuilles, lave et blocs inconnus mélangés.
    fn random_chunk(r: &mut Lcg, ids: &[u8]) -> ChunkVoxels {
        let mut v = ChunkVoxels::filled(0);
        let ground = 20 + r.next(80) as usize;
        for x in 0..CHUNK_SIZE_X {
            for z in 0..CHUNK_SIZE_Z {
                for y in 0..CHUNK_SIZE_Y {
                    let id = if y < ground {
                        if r.next(5) == 0 {
                            ids[r.next(ids.len() as u32) as usize]
                        } else {
                            ids[1]
                        }
                    } else if r.next(40) == 0 {
                        ids[r.next(ids.len() as u32) as usize]
                    } else {
                        0
                    };
                    v.set(x, y, z, id);
                }
            }
        }
        v
    }

    fn check_tables(table: &BlockTable, ids: &[u8], seeds: u64) {
        for seed in 0..seeds {
            let mut r = Lcg(seed);
            let chunks: Vec<ChunkVoxels> = (0..9).map(|_| random_chunk(&mut r, ids)).collect();
            let mut refs: [Option<&ChunkVoxels>; 9] = std::array::from_fn(|i| Some(&chunks[i]));
            if seed % 3 == 1 {
                refs[(seed % 9) as usize] = None;
                refs[4] = Some(&chunks[4]);
            }
            let cache = BlockCache::new(Neighborhood::from_voxels(refs), table);
            let fast = LightVolume::compute_cached(&cache);
            let slow = LightVolume::compute_reference(&cache);
            assert!(fast.light == slow.light, "graine {seed}");
            assert_eq!(fast.atten, slow.atten);
        }
    }

    #[test]
    fn active_diffusion_matches_reference() {
        let reg = BlockRegistry::builtin();
        let ids = reg.ids();
        let table = BlockTable::from_registry(&reg, |_| 0);
        let palette = [
            ids.air,
            ids.stone,
            ids.water,
            ids.lava,
            ids.leaves,
            ids.tall_grass,
            ids.lily_pad,
            ids.sand,
            200,
        ];
        check_tables(&table, &palette, 24);
    }

    #[test]
    fn exotic_table_falls_back_and_matches_reference() {
        // Atténuation 200 (relue −56) et émission 100 : hors du domaine de `diffuse_active`
        // pour la première, dans le domaine (valeurs > 15) pour la seconde.
        let glow = BlockMeshInfo {
            role: BlockRole::Other,
            transparent: true,
            cross_shaped: true,
            grass_tinted: false,
            leaves_tinted: false,
            light_attenuation: 1,
            light_emission: 100,
            texture_layers: [0; 6],
            layer_eighths: 0,
            seasonal: false,
        };
        let mut murky = glow.clone();
        murky.light_emission = 0;
        murky.light_attenuation = 200;
        let mut thick = glow.clone();
        thick.light_emission = 0;
        thick.light_attenuation = 7;
        let stone = BlockMeshInfo {
            cross_shaped: false,
            transparent: false,
            light_emission: 0,
            ..glow.clone()
        };
        let entries = |with_murky: bool| {
            let mut e = vec![
                (0, BlockMeshInfo::air()),
                (1, stone.clone()),
                (2, glow.clone()),
                (3, thick.clone()),
            ];
            if with_murky {
                e.push((4, murky.clone()));
            }
            e
        };
        check_tables(&BlockTable::new(entries(false)), &[0, 1, 2, 3], 6);
        check_tables(&BlockTable::new(entries(true)), &[0, 1, 2, 3, 4], 6);
    }
}
