//! What every unit kind is baked from (CC0 Quaternius characters, assets/CREDITS.md).

pub struct Kind {
    /// Output name: `assets/units/<name>.png` and `.txt`.
    pub name: &'static str,
    /// glTF file in `assets/3d/characters/`.
    pub model: &'static str,
    /// Model units from the feet to the top of the head (34 base px).
    pub head: f32,
    /// Materials drawn in the magenta key (the game swaps it per tribe).
    pub tribe: &'static [&'static str],
    /// sRGB colour of the material named `Skin` (the models ship a near-black one); None keeps it.
    pub skin: Option<&'static str>,
    /// Poses the game draws for this kind (`shots::SHOTS` names).
    pub poses: &'static [&'static str],
}

/// The shaman never strands (she idles), the followers never cast.
const SHAMAN_POSES: &[&str] = &["idle", "walk", "pray", "cast", "fall", "drown"];
const FOLLOWER_POSES: &[&str] = &["idle", "walk", "pray", "fall", "drown", "stranded"];
const SKIN: Option<&str> = Some("d29a6e");

pub const KINDS: &[Kind] = &[
    Kind { name: "shaman", model: "witch.gltf", head: 3.1, tribe: &["Clothes", "Hat"], skin: SKIN, poses: SHAMAN_POSES },
    Kind { name: "brave", model: "worker_male.gltf", head: 3.1, tribe: &["Shirt"], skin: SKIN, poses: FOLLOWER_POSES },
    Kind { name: "warrior", model: "soldier_female.gltf", head: 3.1, tribe: &["Main"], skin: SKIN, poses: FOLLOWER_POSES },
    Kind { name: "preacher", model: "wizard.gltf", head: 3.1, tribe: &["Clothes"], skin: SKIN, poses: FOLLOWER_POSES },
    Kind { name: "spy", model: "ninja_male_hair.gltf", head: 3.1, tribe: &["Details"], skin: SKIN, poses: FOLLOWER_POSES },
    Kind { name: "firewarrior", model: "cowboy_male.gltf", head: 3.1, tribe: &["Jacket"], skin: SKIN, poses: FOLLOWER_POSES },
];

/// The kinds to bake: all, or the one named.
pub fn select(name: Option<&str>) -> Result<Vec<&'static Kind>, String> {
    match name {
        None => Ok(KINDS.iter().collect()),
        Some(n) => KINDS.iter().find(|k| k.name == n).map(|k| vec![k]).ok_or_else(|| {
            let names: Vec<_> = KINDS.iter().map(|k| k.name).collect();
            format!("unknown kind `{n}` (one of {})", names.join(", "))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shots::SHOTS;

    #[test]
    fn every_kind_has_a_model_and_known_poses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/3d/characters");
        for k in KINDS {
            assert!(dir.join(k.model).is_file(), "{}: {}", k.name, k.model);
            assert!(k.poses.iter().all(|p| SHOTS.iter().any(|s| s.pose == *p)), "{}", k.name);
            assert!(k.skin.is_none_or(|s| s.len() == 6 && u32::from_str_radix(s, 16).is_ok()));
        }
    }

    #[test]
    fn select_one_or_all() {
        assert_eq!(select(None).unwrap().len(), KINDS.len());
        assert_eq!(select(Some("spy")).unwrap()[0].model, "ninja_male_hair.gltf");
        assert!(select(Some("wizard")).is_err());
    }
}
