//! Règles de l'eau vivante sur `MemFluid`, sans chunks (doc 07 §15).

use nb_core::glam::IVec3;
use nb_data::{BlockId, BlockRegistry, WaterRules, WorldKind};
use nb_world::fluid::{
    Dir, FluidBlocks, FluidCode, FluidEnv, FluidEvents, FluidSim, MemFluid, RestKind,
    WaterRest,
};

struct T {
    sim: FluidSim,
    w: MemFluid,
    air: BlockId,
    water: BlockId,
    granite: BlockId,
    lava: BlockId,
    quench: BlockId,
    registry: BlockRegistry,
}

fn v(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

fn setup(min: IVec3, max: IVec3, tables: bool) -> T {
    let registry = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let blocks = FluidBlocks::from_registry(&registry);
    let mut rules = (*WaterRules::builtin()).clone();
    rules.time_cap_micros = 0; // déterminisme : pas de plafond de temps
    let (air, water, lava, quench) = (blocks.air, blocks.water, blocks.lava, blocks.quench);
    let granite = registry.get("granit").id;
    let sim = FluidSim::new(std::sync::Arc::new(rules), blocks, 42, tables);
    T {
        sim,
        w: MemFluid::new(min, max, air),
        air,
        water,
        granite,
        lava,
        quench,
        registry,
    }
}

impl T {
    fn run(&mut self, ticks: u32) {
        let env = FluidEnv::default();
        for _ in 0..ticks {
            self.sim.tick(&mut self.w, FluidEvents::default(), &env);
        }
    }

    /// Avance jusqu'au repos (file vide) ; renvoie le nombre de pas écoulés.
    fn settle(&mut self, max_steps: u32) -> u32 {
        let env = FluidEnv::default();
        let mut steps = 0;
        while steps < max_steps {
            for _ in 0..12 {
                self.sim.tick(&mut self.w, FluidEvents::default(), &env);
            }
            steps += 1;
            if self.sim.is_idle() {
                // Un pas de plus pour laisser finir ce qui est en cours.
                for _ in 0..12 {
                    self.sim.tick(&mut self.w, FluidEvents::default(), &env);
                }
                if self.sim.is_idle() {
                    return steps;
                }
            }
        }
        steps
    }

    fn put_water(&mut self, p: IVec3, level: u8) {
        let code = if level == 8 {
            FluidCode::DEFAULT
        } else {
            FluidCode::explicit(level, Dir::None, false)
        };
        self.w.set(p, self.water, code);
        self.sim.wake_around(&mut self.w, p);
    }

    fn lvl(&self, p: IVec3) -> i32 {
        self.sim.lvl(&self.w, p)
    }

    fn volume(&self, min: IVec3, max: IVec3) -> i64 {
        self.sim.total_volume(&self.w, min, max)
    }
}

#[test]
fn conservation_in_a_closed_vat() {
    let (min, max) = (v(0, 0, 0), v(8, 12, 8));
    let mut t = setup(min, max, false);
    // Forme quelconque : quelques blocs de granit dans la cuve.
    for (x, y, z) in [(3, 0, 3), (3, 1, 3), (4, 0, 3), (2, 2, 5), (5, 3, 2)] {
        t.w.set(v(x, y, z), t.granite, FluidCode::DEFAULT);
    }
    let mut total = 0;
    for (i, (x, z)) in [(1, 1), (6, 1), (2, 6), (5, 5), (3, 3)].into_iter().enumerate() {
        for y in 6..10 {
            t.put_water(v(x, y, z), 8);
            total += 8;
        }
        let _ = i;
    }
    for round in 0..40 {
        t.run(120);
        assert_eq!(t.volume(min, max), total, "volume perdu à la ronde {round}");
    }
    t.settle(2000);
    assert_eq!(t.volume(min, max), total);
    assert!(t.sim.is_idle(), "la cuve doit finir au repos");
    // Repos : plus aucune écriture ensuite.
    let writes = t.w.writes;
    t.run(600);
    assert_eq!(t.w.writes, writes);
}

#[test]
fn flat_basin_equalizes() {
    let (min, max) = (v(0, 0, 0), v(8, 6, 8));
    let mut t = setup(min, max, false);
    for (x, z) in [(0, 0), (7, 0), (0, 7), (7, 7)] {
        for y in 0..4 {
            t.put_water(v(x, y, z), 8);
        }
    }
    t.settle(2000);
    let levels: Vec<i32> = (0..8)
        .flat_map(|x| (0..8).map(move |z| (x, z)))
        .map(|(x, z)| t.lvl(v(x, 0, z)))
        .collect();
    let (lo, hi) = (levels.iter().min().unwrap(), levels.iter().max().unwrap());
    assert!(hi - lo <= 1, "niveaux {levels:?}");
    // Un canal d'un bloc de large et de 40 de long aussi.
    let (min, max) = (v(0, 0, 0), v(40, 4, 1));
    let mut t = setup(min, max, false);
    for y in 0..4 {
        t.put_water(v(0, y, 0), 8);
        t.put_water(v(1, y, 0), 8);
    }
    t.settle(3000);
    let levels: Vec<i32> = (0..40).map(|x| t.lvl(v(x, 0, 0))).collect();
    let (lo, hi) = (levels.iter().min().unwrap(), levels.iter().max().unwrap());
    assert!(hi - lo <= 1, "canal : {levels:?}");
    assert_eq!(t.volume(min, max), 64);
}

#[test]
fn falling_column_is_continuous() {
    // Une source infinie sur un socle verse dans une colonne d'air de 9 blocs.
    let mut t = waterfall_world();
    for step in 0..30 {
        t.run(12);
        if step >= 14 {
            for y in 2..=10 {
                assert!(
                    t.lvl(v(1, y, 0)) > 0,
                    "trou dans le filet à y = {y} (pas {step})"
                );
            }
        }
    }
}

#[test]
fn single_blob_reaches_the_floor_in_ten_steps() {
    let (min, max) = (v(0, 0, 0), v(3, 14, 3));
    let mut t = setup(min, max, false);
    t.put_water(v(1, 11, 1), 8);
    t.run(12 * 14);
    assert!(
        (0..3).any(|x| (0..3).any(|z| t.lvl(v(x, 0, z)) > 0)),
        "l'eau n'a pas touché le sol"
    );
    assert_eq!(t.volume(min, max), 8);
}

#[test]
fn puddle_at_a_cliff_edge_empties() {
    let (min, max) = (v(0, 0, 0), v(6, 6, 3));
    let mut t = setup(min, max, false);
    t.w.fill(v(0, 0, 0), v(3, 3, 3), t.granite);
    t.put_water(v(2, 3, 1), 3);
    t.settle(500);
    assert_eq!(t.lvl(v(2, 3, 1)), 0);
    for x in 0..3 {
        for z in 0..3 {
            assert_eq!(t.lvl(v(x, 3, z)), 0, "reste d'eau en ({x}, {z})");
        }
    }
    assert_eq!(t.volume(min, max), 3);
}

#[test]
fn u_tube_levels_match() {
    let (min, max) = (v(0, 0, 0), v(5, 8, 1));
    let mut t = setup(min, max, false);
    t.w.fill(v(1, 1, 0), v(4, 8, 1), t.granite);
    for y in 0..6 {
        t.put_water(v(0, y, 0), 8);
    }
    let start_head = 8 * 5 + 8;
    t.settle(2000);
    let head = |x: i32| {
        (0..8)
            .filter(|&y| t.lvl(v(x, y, 0)) > 0)
            .map(|y| 8 * y + t.lvl(v(x, y, 0)))
            .max()
            .unwrap()
    };
    let (l, r) = (head(0), head(4));
    assert!((l - r).abs() <= 1, "bras gauche {l}, droit {r}");
    assert!(l <= start_head && r <= start_head, "l'eau est montée plus haut que sa source");
    assert_eq!(t.volume(min, max), 48);
}

#[test]
fn a_film_finds_the_edge_and_otherwise_stays() {
    let (min, max) = (v(0, 0, 0), v(8, 4, 1));
    let mut t = setup(min, max, false);
    t.w.fill(v(0, 0, 0), v(6, 1, 1), t.granite);
    t.put_water(v(2, 1, 0), 1);
    t.settle(300);
    assert_eq!(t.lvl(v(2, 1, 0)), 0);
    assert_eq!(t.lvl(v(6, 0, 0)) + t.lvl(v(7, 0, 0)), 1);
    // Sans bord : un film isolé ne bouge pas.
    let mut t = setup(min, max, false);
    t.w.fill(v(0, 0, 0), v(8, 1, 1), t.granite);
    t.put_water(v(2, 1, 0), 1);
    t.settle(300);
    assert_eq!(t.lvl(v(2, 1, 0)), 1);
}

fn sea_world() -> T {
    // Mer x ∈ [0, 4), eau de y = 0 à 4 ; canal d'air à droite sur un plancher de granit.
    let (min, max) = (v(0, 0, 0), v(14, 8, 1));
    let mut t = setup(min, max, true);
    for x in 0..4 {
        for y in 0..5 {
            t.w.set(v(x, y, 0), t.water, FluidCode::DEFAULT);
        }
        t.w.set_rest(
            x,
            0,
            WaterRest {
                top: 0,
                level: 5,
                kind: RestKind::Sea,
                flow: [0, 0],
            },
        );
    }
    t.w.fill(v(4, 0, 0), v(14, 1, 1), t.granite);
    t
}

#[test]
fn a_canal_fills_from_the_sea_and_sleeps() {
    let mut t = sea_world();
    t.sim.wake_around(&mut t.w, v(3, 2, 0));
    t.sim.wake_around(&mut t.w, v(4, 2, 0));
    let steps = t.settle(1500);
    assert!(t.sim.is_idle(), "la file doit se vider ({steps} pas)");
    for x in 4..14 {
        for y in 1..5 {
            assert!(
                t.lvl(v(x, y, 0)) >= 6,
                "canal peu rempli en ({x}, {y}) : {}",
                t.lvl(v(x, y, 0))
            );
        }
    }
    // Rien au-dessus du niveau de la mer.
    for x in 0..14 {
        assert_eq!(t.lvl(v(x, 6, 0)), 0);
    }
}

#[test]
fn the_sea_swallows_what_is_poured_in() {
    let mut t = sea_world();
    t.settle(1500);
    // De l'eau libre posée au-dessus du canal rempli retourne à la mer.
    for x in 4..7 {
        t.put_water(v(x, 5, 0), 8);
    }
    t.settle(1500);
    let mut above = 0;
    for x in 0..14 {
        for y in 5..8 {
            above += t.lvl(v(x, y, 0));
        }
    }
    assert_eq!(above, 0, "il reste de l'eau au-dessus de la mer");
    // Verser dans la mer ne change rien.
    assert_eq!(t.sim.pour(&mut t.w, v(1, 2, 0), 8), 8);
}

#[test]
fn a_pierced_lake_loses_exactly_what_leaves() {
    let (min, max) = (v(0, -6, 0), v(9, 6, 9));
    let mut t = setup(min, max, false);
    t.w.fill(v(0, -6, 0), v(9, 1, 9), t.granite);
    for y in -6..1 {
        t.w.set(v(4, y, 4), t.air, FluidCode::DEFAULT);
    }
    let mut lake = 0;
    for x in 2..7 {
        for z in 2..7 {
            for y in 1..4 {
                t.w.set(v(x, y, z), t.water, FluidCode::DEFAULT);
                lake += 8;
            }
        }
    }
    t.sim.wake_around(&mut t.w, v(4, 1, 4));
    t.settle(3000);
    let in_lake = t.volume(v(0, 1, 0), v(9, 6, 9));
    let below = t.volume(v(0, -6, 0), v(9, 1, 9));
    assert_eq!(in_lake + below, lake as i64, "volume non conservé");
    assert_eq!(below, 7 * 8, "le trou doit se remplir : {below}");
}

fn waterfall_world() -> T {
    // Source infinie en (0, 10, 0) sur un socle de granit, lèvre en (1, 10, 0), chute de
    // 9 blocs dans un puits infini en (1, 0..2, 0).
    let (min, max) = (v(0, 0, 0), v(3, 12, 1));
    let mut t = setup(min, max, true);
    t.w.fill(v(0, 0, 0), v(1, 10, 1), t.granite);
    t.w.set(v(0, 10, 0), t.water, FluidCode::DEFAULT);
    t.w.set_rest(
        0,
        0,
        WaterRest {
            top: 10,
            level: 11,
            kind: RestKind::Sea,
            flow: [0, 0],
        },
    );
    for y in 0..2 {
        t.w.set(v(1, y, 0), t.water, FluidCode::DEFAULT);
    }
    t.w.set_rest(
        1,
        0,
        WaterRest {
            top: 0,
            level: 2,
            kind: RestKind::Sea,
            flow: [0, 0],
        },
    );
    t.sim.wake_around(&mut t.w, v(0, 10, 0));
    t
}

#[test]
fn a_waterfall_becomes_steady_and_wakes_on_edit() {
    let mut t = waterfall_world();
    let steps = t.settle(60);
    assert!(steps < 40, "régime permanent trop lent : {steps} pas");
    assert!(t.sim.is_idle());
    for y in 2..=10 {
        let (_, code) = t.w.get(v(1, y, 0)).unwrap();
        assert!(code.steady(), "cellule (1, {y}) pas en régime permanent");
        assert_eq!(code.dir(), Dir::Down);
    }
    // Casser un bloc sous la chute la réveille.
    let mut ev = FluidEvents::default();
    ev.edits.push(v(1, 5, 0));
    t.sim.tick(&mut t.w, ev, &FluidEnv::default());
    assert!(!t.sim.is_idle() || !t.w.get(v(1, 5, 0)).unwrap().1.steady());
    t.settle(60);
    assert!(t.w.get(v(1, 5, 0)).unwrap().1.steady());
}

#[test]
fn block_classes_behave() {
    let (min, max) = (v(0, 0, 0), v(5, 4, 1));
    let mut t = setup(min, max, false);
    let reg = &t.registry;
    let washable = reg
        .iter()
        .find(|b| b.cross_shaped && b.fluid == nb_data::FluidKind::Auto)
        .expect("un bloc en croix")
        .id;
    let fence = reg.get("staket").id;
    let reed = reg.get("vass").id;
    t.w.fill(v(0, 0, 0), v(5, 1, 1), t.granite);
    t.w.set(v(1, 1, 0), washable, FluidCode::DEFAULT);
    t.w.set(v(2, 1, 0), fence, FluidCode::DEFAULT);
    t.w.set(v(4, 1, 0), reed, FluidCode::DEFAULT);
    // Un vass sans octet compte pour 8.
    assert_eq!(t.lvl(v(4, 1, 0)), 8);
    t.put_water(v(0, 1, 0), 8);
    for y in 2..4 {
        t.put_water(v(0, y, 0), 8);
    }
    t.settle(500);
    // Le bloc emporté a disparu, la clôture garde sa place et se remplit.
    assert_ne!(t.w.get(v(1, 1, 0)).unwrap().0, washable);
    assert_eq!(t.w.get(v(2, 1, 0)).unwrap().0, fence);
    assert!(t.lvl(v(2, 1, 0)) > 0, "clôture non immergée");
    assert_eq!(t.w.get(v(4, 1, 0)).unwrap().0, reed);
}

#[test]
fn water_stops_at_an_unloaded_chunk_and_resumes() {
    let (min, max) = (v(0, 0, 0), v(4, 3, 1));
    let mut t = setup(min, max, false);
    t.w.fill(v(0, 0, 0), v(4, 1, 1), t.granite);
    for y in 1..3 {
        t.put_water(v(3, y, 0), 8);
    }
    t.settle(500);
    assert_eq!(t.volume(min, max), 16, "rien ne doit se perdre au bord");
    // Le chunk voisin se charge.
    t.w.grow(v(0, 0, 0), v(8, 3, 1));
    t.w.fill(v(4, 0, 0), v(8, 1, 1), t.granite);
    for y in 0..3 {
        t.sim.wake_around(&mut t.w, v(3, y, 0));
    }
    t.settle(500);
    let right = t.volume(v(4, 0, 0), v(8, 3, 1));
    assert!(right > 0, "l'eau ne repart pas");
    assert_eq!(t.volume(v(0, 0, 0), v(8, 3, 1)), 16);
}

#[test]
fn water_meets_lava() {
    let (min, max) = (v(0, 0, 0), v(4, 3, 1));
    let mut t = setup(min, max, false);
    t.w.fill(v(0, 0, 0), v(4, 1, 1), t.granite);
    t.w.set(v(2, 1, 0), t.lava, FluidCode::DEFAULT);
    t.put_water(v(0, 1, 0), 8);
    t.put_water(v(0, 2, 0), 8);
    t.settle(300);
    assert_eq!(t.w.get(v(2, 1, 0)).unwrap().0, t.quench);
}

fn scenario(seed_cells: &[(i32, i32, i32)]) -> Vec<u8> {
    let (min, max) = (v(0, 0, 0), v(8, 10, 8));
    let mut t = setup(min, max, false);
    for &(x, y, z) in seed_cells {
        t.put_water(v(x, y, z), 8);
    }
    t.settle(800);
    t.w.bytes()
}

#[test]
fn simulation_is_deterministic() {
    let cells = [(1, 8, 1), (6, 8, 2), (3, 9, 5), (4, 7, 4), (2, 8, 6)];
    assert_eq!(scenario(&cells), scenario(&cells));
}

#[test]
fn tick_budget_is_respected() {
    // Rayon sec très large : 512 cellules par tick.
    let (min, max) = (v(0, 0, 0), v(24, 12, 24));
    let registry = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let blocks = FluidBlocks::from_registry(&registry);
    let air = blocks.air;
    let mut sim = FluidSim::new(WaterRules::builtin(), blocks, 7, false);
    let mut w = MemFluid::new(min, max, air);
    for x in 0..24 {
        for z in 0..24 {
            for y in 6..12 {
                sim.pour(&mut w, v(x, y, z), 8);
            }
        }
    }
    let env = FluidEnv::default();
    let mut total = 0u64;
    let ticks = 240u32;
    for _ in 0..ticks {
        sim.tick(&mut w, FluidEvents::default(), &env);
        total += u64::from(sim.stats.last_tick_us);
        assert!(sim.stats.processed <= 512);
    }
    // Budget : 0,25 ms en moyenne, ×6 en profil dev.
    assert!(total / u64::from(ticks) <= 1500, "tick moyen {} µs", total / u64::from(ticks));
}
