//! Palette de blocs par monde (plan Norrcraft, session 0.3) : identifiants figés par monde,
//! étanchéité des univers, blocs manquants.

use nb_data::blocks::{MISSING_TEXTURE, SOCLE};
use nb_data::{
    BlockPalette, BlockRegistry, ItemRegistry, PaletteReport, RecipeBook, SurvivalData, WorldKind,
    UNASSIGNED_ID,
};

fn palette_of(slots: &[(usize, &str)], len: usize) -> BlockPalette {
    let mut names = vec![String::new(); len];
    for (i, n) in slots {
        names[*i] = (*n).to_owned();
    }
    BlockPalette::new(names)
}

#[test]
fn vanilla_keeps_its_historical_ids_and_the_socle_is_shared() {
    let v = BlockRegistry::for_kind(WorldKind::Vanilla);
    let i = v.ids();
    assert_eq!((i.air, i.stone, i.water, i.bedrock, i.lava), (0, 3, 4, 6, 33));
    assert_eq!(v.get("workbench").id, 200);
    assert_eq!(v.get("sandstone").id, 34);
    let n = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let j = n.ids();
    assert_eq!((j.air, j.water, j.bedrock, j.lava), (0, 4, 6, 33));
    for s in SOCLE {
        assert_eq!(v.get(s), n.get(s), "le socle {s} est identique partout");
    }
}

#[test]
fn universes_do_not_share_blocks_beyond_the_socle() {
    let v = BlockRegistry::for_kind(WorldKind::Vanilla);
    let n = BlockRegistry::for_kind(WorldKind::Norrcraft);
    for b in n.iter() {
        if SOCLE.contains(&b.name.as_str()) {
            continue;
        }
        assert_eq!(
            v.get(&b.name).id,
            0,
            "{} de Norrcraft existe aussi dans vanilla",
            b.name
        );
    }
    for b in v.iter() {
        if SOCLE.contains(&b.name.as_str()) {
            continue;
        }
        assert_eq!(
            n.get(&b.name).id,
            0,
            "{} de vanilla existe aussi dans Norrcraft",
            b.name
        );
    }
    assert_ne!(n.get("granit").id, 0);
    // Le code référence des blocs vanilla : absents de Norrcraft, ils valent « non attribué ».
    assert_eq!(n.ids().stone, UNASSIGNED_ID);
    assert_eq!(n.ids().grass, UNASSIGNED_ID);
    assert!(n.iter().all(|b| b.id != UNASSIGNED_ID));
    assert!(v.iter().all(|b| b.id != UNASSIGNED_ID));
}

#[test]
fn default_palette_round_trips_with_identical_ids() {
    for kind in WorldKind::ALL {
        let reg = BlockRegistry::for_kind(kind);
        let pal = reg.palette();
        assert_eq!(pal.names()[0], "air");
        let (back, report) = BlockRegistry::from_palette(kind, &pal).unwrap();
        assert_eq!(report, PaletteReport::default());
        assert_eq!(back.palette(), pal);
        for b in reg.iter() {
            assert_eq!(back.get(&b.name), b);
        }
    }
}

#[test]
fn palette_fixes_ids_even_when_the_content_order_changes() {
    // Un monde a été créé quand granit était à l'identifiant 40 : il le reste.
    let pal = palette_of(
        &[(0, "air"), (4, "water"), (6, "bedrock"), (33, "lava"), (40, "granit")],
        41,
    );
    let (reg, report) = BlockRegistry::from_palette(WorldKind::Norrcraft, &pal).unwrap();
    assert_eq!(reg.get("granit").id, 40);
    // Les autres blocs de l'univers prennent les premiers identifiants libres, dans l'ordre de
    // la palette par défaut.
    let others: Vec<String> = BlockRegistry::for_kind(WorldKind::Norrcraft)
        .palette()
        .names()
        .iter()
        .filter(|n| !n.is_empty() && *n != "granit" && !SOCLE.contains(&n.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        report.added[..8],
        ["moran", "grus", "flygsand", "klappersten", "torv", "permafrost", "blais", "basaltsten"]
    );
    assert_eq!(report.added, others);
    assert_eq!(reg.get("moran").id, 1);
    assert_eq!(reg.get("grus").id, 2);
    assert!(report.unknown.is_empty());
    // La palette reconstruite se relit à l'identique.
    let (again, rep2) =
        BlockRegistry::from_palette(WorldKind::Norrcraft, &reg.palette()).unwrap();
    assert_eq!(again.palette(), reg.palette());
    assert_eq!(rep2, PaletteReport::default());
}

#[test]
fn unknown_names_become_visible_missing_blocks_that_keep_their_slot() {
    let mut names = BlockRegistry::for_kind(WorldKind::Norrcraft)
        .palette()
        .names()
        .to_vec();
    names.resize(21, String::new());
    names[20] = "bloc_disparu".into();
    let (reg, report) =
        BlockRegistry::from_palette(WorldKind::Norrcraft, &BlockPalette::new(names)).unwrap();
    assert_eq!(report.unknown, ["bloc_disparu"]);
    let missing = reg.from_id(20);
    assert_eq!(missing.name, "bloc_disparu");
    assert!(missing.solid && !missing.transparent);
    assert_eq!(missing.texture_names[0], MISSING_TEXTURE);
    // Le nom est conservé à la sauvegarde suivante.
    assert_eq!(reg.palette().names()[20], "bloc_disparu");
}

#[test]
fn bad_palettes_are_refused() {
    let k = WorldKind::Norrcraft;
    let refused = |p: BlockPalette| BlockRegistry::from_palette(k, &p).is_err();
    assert!(refused(BlockPalette::default()));
    assert!(refused(palette_of(&[(1, "air")], 2)), "air doit être à 0");
    assert!(
        refused(BlockPalette::new(vec![
            "air".into(),
            "granit".into(),
            "granit".into()
        ])),
        "doublon"
    );
    assert!(
        refused(palette_of(&[(0, "air"), (255, "granit")], 256)),
        "l'identifiant 255 est réservé"
    );
}

#[test]
fn fingerprint_ignores_trailing_free_slots_but_not_content() {
    let a = BlockRegistry::for_kind(WorldKind::Norrcraft).palette();
    let mut padded = a.names().to_vec();
    padded.extend(vec![String::new(); 5]);
    assert_eq!(BlockPalette::new(padded).fingerprint(), a.fingerprint());
    let mut renamed = a.names().to_vec();
    renamed[1] = "autre".into();
    assert_ne!(BlockPalette::new(renamed).fingerprint(), a.fingerprint());
    assert_ne!(
        BlockRegistry::for_kind(WorldKind::Vanilla)
            .palette()
            .fingerprint(),
        a.fingerprint()
    );
}

#[test]
fn norrcraft_registries_hold_nothing_from_vanilla() {
    let blocks = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let items = ItemRegistry::for_kind(WorldKind::Norrcraft, &blocks);
    // Un objet par bloc (hors air), aucun objet pur.
    assert_eq!(items.len(), blocks.len() - 1);
    assert!(items.id_of("stick").is_none() && items.id_of("stone").is_none());
    let book = RecipeBook::for_kind(WorldKind::Norrcraft, &items);
    assert_eq!(book.recipes().len(), 0);
    let survival = SurvivalData::for_kind(WorldKind::Norrcraft, &blocks, &items);
    assert!(survival.block_rule(blocks.get("granit").id).is_none());
    let mobs = nb_data::MobRegistry::builtin_for(WorldKind::Norrcraft);
    // Le bestiaire Norrcraft (phase 5) ne reprend aucun mob de vanilla.
    assert!(!mobs.all().is_empty());
    let vanilla = nb_data::MobRegistry::builtin_for(WorldKind::Vanilla);
    assert!(mobs.all().iter().all(|m| vanilla.get(&m.id).is_none()));
    assert_eq!(mobs.kind(), WorldKind::Norrcraft);
    assert!(!nb_data::MobRegistry::builtin_for(WorldKind::Vanilla).all().is_empty());
}
