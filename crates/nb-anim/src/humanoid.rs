//! Rig de l'avatar des autres joueurs (13 boîtes de `nb_gfx::player_boxes`, dans cet ordre :
//! jambes, chaussures, torse, bras, mains, tête, cheveux, yeux).

use crate::def::*;

const S: f32 = 0.9; // même facteur que `player_boxes`

fn bone(name: &str, parent: Option<&str>, pivot: [f32; 3]) -> BoneDef {
    BoneDef {
        name: name.into(),
        parent: parent.map(Into::into),
        pivot: [pivot[0] * S, pivot[1] * S, pivot[2] * S],
    }
}

fn rx(bone: &str, c: Curve) -> ChannelDef {
    ChannelDef {
        rx: Some(c),
        ..ChannelDef::new(bone)
    }
}

fn wave(a: f32, p: f32) -> Curve {
    Curve::Wave(Wave::sin(a, 1.0, p))
}

fn clip(duration: f32, looping: bool, additive: bool, channels: Vec<ChannelDef>) -> ClipDef {
    ClipDef {
        duration,
        looping,
        additive,
        fade: [0.12, 0.2],
        channels,
    }
}

/// Rig de l'avatar (marche, course, repos, saut, blessure).
pub fn humanoid() -> RigDef {
    let mut def = RigDef {
        bones: vec![
            bone("root", None, [0.0, 0.0, 0.0]),
            bone("body", Some("root"), [0.0, 12.0, 0.0]),
            bone("head", Some("body"), [0.0, 24.0, 0.0]),
            bone("arm_l", Some("body"), [-6.0, 24.0, 0.0]),
            bone("arm_r", Some("body"), [6.0, 24.0, 0.0]),
            bone("leg_l", Some("root"), [-2.0, 12.0, 0.0]),
            bone("leg_r", Some("root"), [2.0, 12.0, 0.0]),
        ],
        // Jambes, chaussures, torse, bras, mains, tête, cheveux, yeux.
        boxes: vec![5, 6, 5, 6, 1, 3, 4, 3, 4, 2, 2, 2, 2],
        ..RigDef::default()
    };
    let gait = |a: f32, arm: f32, bob: f32| {
        let mut body = ChannelDef::new("body");
        body.ty = Some(Curve::Wave(Wave {
            abs: true,
            o: 0.0,
            ..Wave::sin(bob, 2.0, 0.0)
        }));
        vec![
            rx("leg_l", wave(a, 0.0)),
            rx("leg_r", wave(a, 0.5)),
            rx("arm_l", wave(arm, 0.5)),
            rx("arm_r", wave(arm, 0.0)),
            body,
        ]
    };
    def.clips
        .insert("walk".into(), clip(0.65, true, false, gait(38.0, 30.0, 0.5)));
    def.clips
        .insert("run".into(), clip(0.5, true, false, gait(55.0, 50.0, 0.9)));
    let mut breathe = ChannelDef::new("body");
    breathe.sy = Some(Curve::Wave(Wave {
        o: 1.0,
        ..Wave::sin(0.012, 1.0, 0.0)
    }));
    let mut sway_l = rx("arm_l", wave(2.5, 0.0));
    sway_l.rz = Some(Curve::Wave(Wave {
        o: -2.0,
        ..Wave::sin(1.5, 1.0, 0.0)
    }));
    let mut sway_r = rx("arm_r", wave(2.5, 0.5));
    sway_r.rz = Some(Curve::Wave(Wave {
        o: 2.0,
        ..Wave::sin(1.5, 1.0, 0.5)
    }));
    def.clips
        .insert("idle".into(), clip(3.2, true, false, vec![breathe, sway_l, sway_r]));
    def.clips.insert(
        "air".into(),
        clip(
            1.0,
            true,
            false,
            vec![
                rx("leg_l", Curve::Const(24.0)),
                rx("leg_r", Curve::Const(-14.0)),
                rx("arm_l", Curve::Const(-35.0)),
                rx("arm_r", Curve::Const(-35.0)),
            ],
        ),
    );
    def.clips.insert(
        "hurt".into(),
        clip(
            0.4,
            false,
            true,
            vec![
                rx("body", Curve::Keys(vec![[0.0, 0.0], [0.25, -14.0], [1.0, 0.0]])),
                rx("head", Curve::Keys(vec![[0.0, 0.0], [0.25, 12.0], [1.0, 0.0]])),
            ],
        ),
    );
    def.gaits = vec![
        GaitDef {
            clip: "walk".into(),
            speed: 4.3,
        },
        GaitDef {
            clip: "run".into(),
            speed: 5.7,
        },
    ];
    def.idle = Some("idle".into());
    def.air = Some("air".into());
    def.actions.insert("hurt".into(), "hurt".into());
    def.look = Some(LookDef {
        bones: vec![LookBone {
            bone: "head".into(),
            weight: 1.0,
        }],
        yaw: 70.0,
        pitch: 70.0,
        range: 0.0,
    });
    def
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Animator, Mat34, Rig, Scratch};

    #[test]
    fn humanoid_compiles_and_animates() {
        let rig = Rig::compile(&humanoid()).unwrap();
        assert_eq!(rig.box_bone.len(), 13);
        let mut a = Animator::new(1);
        let mut sc = Scratch::new();
        let mut out = vec![Mat34::IDENTITY; rig.bone_count()];
        for _ in 0..120 {
            a.update(
                &rig,
                &crate::AnimInput {
                    speed: 4.3,
                    grounded: true,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        a.evaluate(&rig, &mut sc, &mut out);
        assert!(out.iter().all(|m| m.0.iter().all(|v| v.is_finite())));
        assert_ne!(out[5], Mat34::IDENTITY, "les jambes bougent en marchant");
    }
}
