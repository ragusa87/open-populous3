//! Firestorm's and Volcano's own fireball shot (`game_core::panicking_blast`), the "rain of fire" look
//! (pop3-rev-analysis.md "Firestorm fireballs", TODO.md) that was still missing: a bright ball falling straight
//! down from `FALL_HEIGHT`, trailing a few warm embers behind it (not `smoke.rs`'s grey puffs), landing in a
//! quick flash that fades into a last puff of smoke. Our own invented flight (the original's own speed/fall-distance
//! numbers are read but not followed exactly, same spirit as `volcano_rocks`'s own invented ejection):
//! `panicking_blast`'s own effect lives a little longer than its 3-turn blast (`panicking_blast::VISUAL_TICKS`)
//! purely so this view has room to finish — the blast's own damage/push/panic read `self.turn` against `RADII`,
//! already a no-op past turn 3, so the extra idle ticks change nothing about what it does to the world.

use crate::grounded::Grounded;
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

/// How high above the ground the ball starts (cells).
const FALL_HEIGHT: f32 = 2.4;
/// Seconds of fall, then how much longer the landing flash/smoke takes to fade away.
const FALL_SECS: f32 = 0.4;
const BURST_SECS: f32 = 0.55;

/// The ball's size (cells) at the top of its fall and just before landing: it shrinks as it falls
/// (pop3-rev-analysis.md "Firestorm fireballs": the original's own sprite shrinks the same way, "down to a single
/// pixel").
const BALL_SIZE: (f32, f32) = (0.22, 0.05);
const BALL_COLOUR: Color = Color::srgb(1.0, 0.65, 0.2);
const BALL_GLOW: f32 = 5.0;

/// Trailing embers behind the ball (not `smoke.rs`'s grey puffs): how many, how far behind (seconds) and how
/// much smaller/dimmer than the ball each one sits.
const TRAIL: usize = 3;
const TRAIL_GAP: f32 = 0.08;
const TRAIL_SCALE: f32 = 0.5;
const TRAIL_OPACITY: f32 = 0.6;
const EMBER_COLOUR: Color = Color::srgb(0.9, 0.4, 0.1);
const EMBER_GLOW: f32 = 2.5;

/// The landing burst: its size at the flash and once it's grown into its last puff of smoke, and the colour it
/// fades from (a bright flash) towards (smoke).
const BURST_SIZE: (f32, f32) = (0.18, 0.9);
const FLASH_COLOUR: Color = Color::srgb(1.0, 0.8, 0.45);
const SMOKE_COLOUR: Color = Color::srgb(0.25, 0.22, 0.2);

/// The ball's height above ground and size `secs` into its fall; `None` once it's landed (gravity-like: slow at
/// first, fast by the end).
pub fn ball(secs: f32) -> Option<(f32, f32)> {
    (0.0..FALL_SECS).contains(&secs).then(|| {
        let p = secs / FALL_SECS;
        (FALL_HEIGHT * (1.0 - p * p), BALL_SIZE.0 + (BALL_SIZE.1 - BALL_SIZE.0) * p)
    })
}

/// The burst's size and opacity `secs` after it landed (`secs - FALL_SECS`); `None` once over.
pub fn burst(secs: f32) -> Option<(f32, f32)> {
    let t = secs - FALL_SECS;
    (0.0..BURST_SECS).contains(&t).then(|| {
        let p = t / BURST_SECS;
        (BURST_SIZE.0 + (BURST_SIZE.1 - BURST_SIZE.0) * p.sqrt(), (1.0 - p).powi(2))
    })
}

#[derive(Resource)]
pub struct FireballAssets {
    sphere: Handle<Mesh>,
    ball_material: Handle<StandardMaterial>,
    trail_material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct FireballPart;

/// One fireball shot's view: its falling ball, its trailing embers and its landing burst — each its own entity
/// and material (the burst's and embers' fade on their own).
#[derive(Component)]
pub struct FireballShotView {
    ball: Entity,
    trail: [Entity; TRAIL],
    burst: Entity,
    burst_material: Handle<StandardMaterial>,
}

impl WorldView for FireballShotView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, FireballAssets>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility), With<FireballPart>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::PanickingBlast(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        let (mut ball, mut trail, mut burst) = (Entity::PLACEHOLDER, [Entity::PLACEHOLDER; TRAIL], Entity::PLACEHOLDER);
        let burst_material = mats.add(StandardMaterial { base_color: FLASH_COLOUR, alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() });
        view.with_children(|v| {
            ball = v.spawn((FireballPart, Mesh3d(assets.sphere.clone()), MeshMaterial3d(assets.ball_material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
            for t in &mut trail {
                *t = v
                    .spawn((FireballPart, Mesh3d(assets.sphere.clone()), MeshMaterial3d(assets.trail_material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden))
                    .id();
            }
            burst = v.spawn((FireballPart, Mesh3d(assets.sphere.clone()), MeshMaterial3d(burst_material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
        });
        Some(FireballShotView { ball, trail, burst, burst_material })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, mats, parts): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        for (k, &part) in std::iter::once(&self.ball).chain(self.trail.iter()).enumerate() {
            let Ok((mut t, mut vis)) = parts.get_mut(part) else { continue };
            match ball(secs - k as f32 * TRAIL_GAP) {
                Some((height, size)) => {
                    let scale = if k == 0 { size } else { size * TRAIL_SCALE };
                    (t.translation, t.scale, *vis) = (Vec3::Y * height, Vec3::splat(scale), Visibility::Inherited);
                }
                None => *vis = Visibility::Hidden,
            }
        }
        let Ok((mut t, mut vis)) = parts.get_mut(self.burst) else { return };
        match burst(secs) {
            Some((size, opacity)) => {
                (t.scale, *vis) = (Vec3::splat(size), Visibility::Inherited);
                if let Some(mut m) = mats.get_mut(&self.burst_material) {
                    let p = ((secs - FALL_SECS) / BURST_SECS).clamp(0.0, 1.0);
                    m.base_color = FLASH_COLOUR.mix(&SMOKE_COLOUR, p).with_alpha(opacity);
                }
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

pub struct FireballShotPlugin;

impl Plugin for FireballShotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<FireballShotView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let ball_material = mats.add(StandardMaterial { base_color: BALL_COLOUR, emissive: LinearRgba::from(BALL_COLOUR) * BALL_GLOW, unlit: true, ..default() });
    let trail_material = mats.add(StandardMaterial {
        base_color: EMBER_COLOUR.with_alpha(TRAIL_OPACITY),
        emissive: LinearRgba::from(EMBER_COLOUR) * EMBER_GLOW,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(FireballAssets { sphere: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()), ball_material, trail_material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ball_falls_fast_then_lands_shrinking_on_the_way() {
        assert_eq!(ball(-0.01), None, "not out yet");
        let (high, big) = ball(0.0).unwrap();
        assert_eq!((high, big), (FALL_HEIGHT, BALL_SIZE.0));
        let (mid, _) = ball(FALL_SECS * 0.5).unwrap();
        assert!(mid > 0.0 && mid < FALL_HEIGHT, "still falling: {mid}");
        assert!(ball(FALL_SECS - 0.001).unwrap().0 < mid, "faster near the ground: {mid}");
        assert_eq!(ball(FALL_SECS), None, "landed");
    }

    #[test]
    fn the_burst_flashes_then_grows_and_fades_into_smoke() {
        assert_eq!(burst(FALL_SECS - 0.001), None, "not landed yet");
        let (small, bright) = burst(FALL_SECS).unwrap();
        assert_eq!((small, bright), (BURST_SIZE.0, 1.0));
        let (bigger, dimmer) = burst(FALL_SECS + BURST_SECS * 0.5).unwrap();
        assert!(bigger > small && dimmer < bright);
        assert_eq!(burst(FALL_SECS + BURST_SECS), None, "over");
    }
}
