//! Dev tuning: how big objects are drawn against the cells, per group (F2 menu, `POP3_SCALE_<GROUP>`).
//! Each object's grounded root carries `Scaled`, and its whole model scales about the point it stands on.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectGroup {
    Trees,
    Units,
    Buildings,
    Scenery,
}

impl ObjectGroup {
    pub const ALL: [ObjectGroup; 4] = [ObjectGroup::Trees, ObjectGroup::Units, ObjectGroup::Buildings, ObjectGroup::Scenery];

    pub fn name(self) -> &'static str {
        match self {
            ObjectGroup::Trees => "Trees",
            ObjectGroup::Units => "Units",
            ObjectGroup::Buildings => "Buildings",
            ObjectGroup::Scenery => "Scenery",
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            ObjectGroup::Trees => "POP3_SCALE_TREES",
            ObjectGroup::Units => "POP3_SCALE_UNITS",
            ObjectGroup::Buildings => "POP3_SCALE_BUILDINGS",
            ObjectGroup::Scenery => "POP3_SCALE_SCENERY",
        }
    }
}

/// The root of an object drawn at its group's scale.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scaled(pub ObjectGroup);

pub const SCALE_RANGE: (f32, f32) = (0.25, 4.0);

/// Units are drawn 1.5 times their sprites' size, as in the original (by eye).
pub const UNITS_SCALE: f32 = 1.5;

/// Drawing scale per group, 1 = the original model size (512 world units per cell); units `UNITS_SCALE` by default.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ObjectScale([f32; 4]);

impl Default for ObjectScale {
    fn default() -> Self {
        ObjectScale::from_env(|k| std::env::var(k).ok())
    }
}

impl ObjectScale {
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut s = ObjectScale([1.0; 4]);
        s.set(ObjectGroup::Units, UNITS_SCALE);
        for g in ObjectGroup::ALL {
            if let Some(v) = get(g.env_key()).and_then(|v| v.parse::<f32>().ok()) {
                s.set(g, v);
            }
        }
        s
    }

    pub fn get(&self, group: ObjectGroup) -> f32 {
        self.0[group as usize]
    }

    pub fn set(&mut self, group: ObjectGroup, value: f32) {
        self.0[group as usize] = value.clamp(SCALE_RANGE.0, SCALE_RANGE.1);
    }

    /// One line for the log and the menu.
    pub fn describe(&self) -> String {
        ObjectGroup::ALL.iter().map(|&g| format!("{} x{:.2}", g.name().to_lowercase(), self.get(g))).collect::<Vec<_>>().join("  ")
    }
}

pub struct ObjectScalePlugin;

impl Plugin for ObjectScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectScale>().add_systems(PostUpdate, apply_scale.before(TransformSystems::Propagate));
    }
}

fn apply_scale(scale: Res<ObjectScale>, mut q: Query<(Ref<Scaled>, &mut Transform)>) {
    for (s, mut t) in &mut q {
        let wanted = Vec3::splat(scale.get(s.0));
        if (scale.is_changed() || s.is_added()) && t.scale != wanted {
            t.scale = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_start_at_their_defaults_read_the_env_and_clamp() {
        let get = |k: &str| match k {
            "POP3_SCALE_TREES" => Some("1.5".to_string()),
            "POP3_SCALE_UNITS" => Some("oops".to_string()),
            "POP3_SCALE_BUILDINGS" => Some("99".to_string()),
            _ => None,
        };
        let s = ObjectScale::from_env(get);
        assert_eq!(s.get(ObjectGroup::Trees), 1.5);
        assert_eq!(s.get(ObjectGroup::Units), UNITS_SCALE, "bad value ignored");
        assert_eq!(s.get(ObjectGroup::Buildings), SCALE_RANGE.1, "clamped");
        assert_eq!(s.get(ObjectGroup::Scenery), 1.0);
        assert_eq!(ObjectScale::from_env(|_| None).describe(), "trees x1.00  units x1.50  buildings x1.00  scenery x1.00");
    }

    #[test]
    fn a_scaled_root_follows_its_group() {
        let mut app = App::new();
        app.insert_resource(ObjectScale::from_env(|_| None)).add_systems(Update, apply_scale);
        let tree = app.world_mut().spawn((Scaled(ObjectGroup::Trees), Transform::default())).id();
        let hut = app.world_mut().spawn((Scaled(ObjectGroup::Buildings), Transform::default())).id();
        app.world_mut().resource_mut::<ObjectScale>().set(ObjectGroup::Trees, 2.0);
        app.update();
        assert_eq!(app.world().get::<Transform>(tree).unwrap().scale, Vec3::splat(2.0));
        assert_eq!(app.world().get::<Transform>(hut).unwrap().scale, Vec3::ONE);
    }
}
