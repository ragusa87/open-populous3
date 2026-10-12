//! The spell range ring (UX only): while a ground spell with a range is aimed, a glowing curtain stands around the
//! player's shaman at that range (`GameMap::cast_range`), growing in with a small overshoot and shrinking back into
//! her when the spell is put away; when she starts casting, a white line contracts from the ring into her and the
//! curtain jumps up. Our own look (an "aurora" in gold), not the original's: kept in this file alone, reading the
//! selected spell and the map, so it can be redone or dropped without touching anything else. On `Res<Time>`, like
//! the rest of the mouse feedback.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{ground_point, Grounded};
use crate::hud::spells::{ground_spell, SelectedSpell};
use crate::units::{UnitView, PLAYER};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::unit::{Action, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;
use std::f32::consts::{PI, TAU};

/// The recipe was tuned around a shaman 1.3 tall; ours is `units::selection::UNIT_HEIGHT` (0.4 cells).
const LAB: f32 = 0.4 / 1.3;
/// Seconds to grow in, and to shrink out.
const ENVELOPE_SECS: f32 = 0.75;
/// The curtain's swell speed, height and the line's glow.
const SPEED: f32 = 0.32;
const RISE: f32 = 1.03;
const GLOW: f32 = 1.09;
/// Lift above the ground, cells.
const LIFT: f32 = 0.07 * LAB;
/// Columns of the curtain and segments of the line.
const CURTAIN_SEGMENTS: usize = 120;
const LINE_SEGMENTS: usize = 160;
/// Half width of the line on the ground, cells.
const LINE_HALF: f32 = 0.06 * GLOW;
/// The burst on casting, seconds, and how much higher the curtain stands at its start.
const BURST_SECS: f32 = 0.9;
const BURST_BOOST: f32 = 1.3;
/// Gold, pale gold, amber.
const PALETTE: [[u8; 3]; 3] = [[0xff, 0xd6, 0x6b], [0xff, 0xf3, 0xc4], [0xff, 0xb0, 0x3b]];

/// Overshoots about 10 % then settles at 1.
pub fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    let u = t - 1.0;
    1.0 + (C1 + 1.0) * u * u * u + C1 * u * u
}

/// Shown (1) or hidden (0), and how far between.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Envelope(pub f32);

impl Envelope {
    pub fn step(&mut self, on: bool, dt: f32) {
        let change = dt / ENVELOPE_SECS;
        self.0 = (self.0 + if on { change } else { -change }).clamp(0.0, 1.0);
    }

    /// Of the full radius.
    pub fn scale(self) -> f32 {
        ease_out_back(self.0)
    }

    /// Opaque after the first third.
    pub fn alpha(self) -> f32 {
        (self.0 * 3.0).min(1.0)
    }
}

/// The burst after a cast: 1 when it starts, 0 when over.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Burst(pub f32);

impl Burst {
    pub fn step(&mut self, dt: f32) {
        self.0 = (self.0 - dt / BURST_SECS).max(0.0);
    }

    /// The white line's radius: from the ring into the caster, faster at the end.
    pub fn radius(self, ring: f32) -> f32 {
        let p = 1.0 - self.0;
        ring * (1.0 - p * p)
    }

    pub fn boost(self) -> f32 {
        1.0 + self.0 * BURST_BOOST
    }
}

/// The curtain's height (cells) at `angle`, `secs` in: a 3-lobe swell and a 7-lobe ripple going opposite ways.
pub fn curtain_height(angle: f32, secs: f32, boost: f32) -> f32 {
    LAB * RISE.max(0.1) * (1.6 + 0.8 * (3.0 * angle + 2.0 * SPEED * secs).sin() + 0.5 * (7.0 * angle - 1.7 * secs).sin()) * boost
}

fn colour(rgb: [u8; 3]) -> Vec3 {
    let c = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

/// The base colour at `angle`: the palette in 6 arcs round the ring, blended from one to the next.
pub fn palette_at(angle: f32) -> Vec3 {
    let arcs = PALETTE.len() * 2;
    let f = angle.rem_euclid(TAU) / TAU * arcs as f32;
    let (i, t) = (f.floor() as usize % arcs, f.fract());
    colour(PALETTE[i % PALETTE.len()]).lerp(colour(PALETTE[(i + 1) % PALETTE.len()]), t)
}

#[derive(Debug, Default, PartialEq)]
pub struct RingMesh {
    pub positions: Vec<[f32; 3]>,
    pub colours: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl RingMesh {
    /// Strips of `rows` vertices per column, joined column to column round the ring.
    fn strips(&mut self, columns: usize, rows: usize) {
        for c in 0..columns as u32 - 1 {
            for r in 0..rows as u32 - 1 {
                let a = c * rows as u32 + r;
                let b = a + rows as u32;
                self.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
}

/// The curtain: `CURTAIN_SEGMENTS` quads round the ring standing on `base(angle)` (render space), fading upwards
/// from the base colour to clear through the pale gold.
pub fn curtain_mesh(base: impl Fn(f32) -> Vec3, secs: f32, boost: f32, alpha: f32) -> RingMesh {
    let mut m = RingMesh::default();
    let pale = colour(PALETTE[1]);
    for k in 0..=CURTAIN_SEGMENTS {
        let angle = k as f32 / CURTAIN_SEGMENTS as f32 * TAU;
        let (foot, h) = (base(angle), curtain_height(angle, secs, boost));
        for (v, rgb, a) in [(0.0, palette_at(angle), 0.40), (0.5, pale, 0.14), (1.0, pale, 0.0)] {
            m.positions.push((foot + Vec3::Y * h * v).into());
            m.colours.push([rgb.x, rgb.y, rgb.z, a * alpha]);
        }
    }
    m.strips(CURTAIN_SEGMENTS + 1, 3);
    m
}

/// A glowing line lying on the ground round `ring(angle, offset)` (render space, `offset` cells outwards): a
/// haze, a glow and a near-white core across it.
pub fn line_mesh(ring: impl Fn(f32, f32) -> Vec3, rgb: Vec3, alpha: f32) -> RingMesh {
    let mut m = RingMesh::default();
    let core = rgb.lerp(Vec3::ONE, 0.65);
    let across = [(-1.0, rgb, 0.0), (-0.3, rgb, 0.15), (0.0, core, 0.9), (0.3, rgb, 0.15), (1.0, rgb, 0.0)];
    for k in 0..=LINE_SEGMENTS {
        let angle = k as f32 / LINE_SEGMENTS as f32 * TAU;
        for (o, c, a) in across {
            m.positions.push(ring(angle, o * LINE_HALF).into());
            m.colours.push([c.x, c.y, c.z, a * 0.8 * alpha]);
        }
    }
    m.strips(LINE_SEGMENTS + 1, across.len());
    m
}

fn append(into: &mut RingMesh, more: RingMesh) {
    let base = into.positions.len() as u32;
    into.positions.extend(more.positions);
    into.colours.extend(more.colours);
    into.indices.extend(more.indices.into_iter().map(|i| i + base));
}

pub struct RangeRingPlugin;

impl Plugin for RangeRingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ring>()
            .add_systems(Startup, spawn_ring)
            .add_systems(Update, follow_aim.in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, draw_ring.before(TransformSystems::Propagate));
    }
}

#[derive(Resource, Default)]
struct Ring {
    envelope: Envelope,
    burst: Burst,
    /// The last range shown (cells), kept while shrinking out.
    radius: f32,
    was_casting: bool,
}

#[derive(Component)]
struct RingPart;

/// The curtain and the line, given their mesh the first frame they are drawn (an empty mesh logs errors).
fn spawn_ring(mut commands: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    let material = mats.add(StandardMaterial { alpha_mode: AlphaMode::Add, unlit: true, cull_mode: None, ..default() });
    for _ in 0..2 {
        commands.spawn((RingPart, MeshMaterial3d(material.clone()), NotShadowCaster, Transform::default(), Visibility::Hidden));
    }
}

/// Shows the ring while a ground spell with a range is selected, starts the burst when the shaman starts casting.
fn follow_aim(time: Res<Time>, selected: Res<SelectedSpell>, map: Res<CurrentMap>, mut ring: ResMut<Ring>) {
    let range = selected.0.filter(|&k| ground_spell(k, (0, 0), (0, 0)).is_some()).and_then(|k| map.0.cast_range(PLAYER, k));
    if let Some(r) = range {
        ring.radius = r as f32 / WORLD_UNITS_PER_CELL as f32;
    }
    let casting = map.0.shaman_of(PLAYER).is_some_and(|u| matches!(u.action, Action::Casting { .. }));
    if casting && !ring.was_casting && ring.envelope.0 > 0.0 {
        ring.burst = Burst(1.0);
    }
    ring.was_casting = casting;
    let dt = time.delta_secs();
    ring.envelope.step(range.is_some(), dt);
    ring.burst.step(dt);
}

#[allow(clippy::too_many_arguments)]
fn draw_ring(
    mut commands: Commands,
    time: Res<Time>,
    ring: Res<Ring>,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    views: Query<(&UnitView, &Grounded)>,
    mut parts: Query<(Entity, Option<&Mesh3d>, &mut Visibility), With<RingPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let shaman = map.0.units.iter().position(|u| u.owner == PLAYER && u.kind == UnitKind::Shaman);
    let centre = shaman.and_then(|i| views.iter().find(|(v, _)| v.0 == i)).map(|(_, g)| g.at);
    let shown = ring.envelope.0 > 0.0 || ring.burst.0 > 0.0;
    let (Some(centre), true) = (centre, shown) else {
        parts.iter_mut().for_each(|(_, _, mut v)| *v = Visibility::Hidden);
        return;
    };
    let terrain = &map.0.terrain;
    let radius = ring.radius * ring.envelope.scale();
    let on_ground = |r: f32, angle: f32| ground_point(terrain, rig.focus, &params.0, centre + Vec2::from_angle(angle - PI / 2.0) * r) + Vec3::Y * LIFT;
    let curtain = curtain_mesh(|a| on_ground(radius, a), time.elapsed_secs(), ring.burst.boost(), ring.envelope.alpha());
    let mut line = line_mesh(|a, o| on_ground(radius + o, a), colour(PALETTE[0]), ring.envelope.alpha());
    if ring.burst.0 > 0.0 {
        append(&mut line, line_mesh(|a, o| on_ground((ring.burst.radius(ring.radius) + o).max(0.0), a), Vec3::ONE, ring.burst.0));
    }
    for ((entity, mesh, mut visibility), part) in parts.iter_mut().zip([curtain, line]) {
        *visibility = Visibility::Visible;
        put(&mut commands, entity, mesh, &mut meshes, part);
    }
}

fn put(commands: &mut Commands, entity: Entity, mesh: Option<&Mesh3d>, meshes: &mut Assets<Mesh>, part: RingMesh) {
    let fill = |m: &mut Mesh| {
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; part.positions.len()]);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, part.positions.clone());
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, part.colours.clone());
        m.insert_indices(Indices::U32(part.indices.clone()));
    };
    if let Some(mut m) = mesh.and_then(|h| meshes.get_mut(&h.0)) {
        fill(&mut m);
        return;
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    fill(&mut m);
    commands.entity(entity).insert(Mesh3d(meshes.add(m)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_with_an_overshoot_and_shrinks_back() {
        assert_eq!((ease_out_back(0.0), ease_out_back(1.0)), (0.0, 1.0));
        assert!((1..10).any(|i| ease_out_back(i as f32 / 10.0) > 1.05), "overshoots");
        let mut e = Envelope::default();
        e.step(true, 0.25);
        assert!((e.0 - 1.0 / 3.0).abs() < 1e-6 && e.alpha() > 0.99, "opaque after a third");
        e.step(true, 1.0);
        assert_eq!(e.0, 1.0);
        e.step(false, ENVELOPE_SECS);
        assert_eq!((e.0, e.scale()), (0.0, 0.0), "gone in the same time");
    }

    #[test]
    fn the_burst_contracts_into_the_caster() {
        let mut b = Burst(1.0);
        assert_eq!((b.radius(6.0), b.boost()), (6.0, 1.0 + BURST_BOOST));
        b.step(BURST_SECS);
        assert_eq!((b.radius(6.0), b.boost()), (0.0, 1.0));
    }

    #[test]
    fn the_curtain_breathes_within_bounds() {
        for k in 0..100 {
            let h = curtain_height(k as f32 * 0.37, k as f32 * 0.11, 1.0);
            assert!((0.29 * RISE * LAB..=2.91 * RISE * LAB).contains(&h), "{h}");
        }
        assert!(curtain_height(0.5, 0.0, 1.0) != curtain_height(0.5, 1.0, 1.0), "moves");
    }

    #[test]
    fn the_palette_goes_round_in_six_arcs() {
        assert_eq!(palette_at(0.0), colour(PALETTE[0]));
        assert_eq!(palette_at(TAU / 6.0), colour(PALETTE[1]));
        assert_eq!(palette_at(TAU / 2.0), colour(PALETTE[0]));
        assert!((palette_at(TAU) - palette_at(0.0)).length() < 1e-5, "closes");
    }

    #[test]
    fn curtain_and_line_meshes() {
        let flat = |a: f32| Vec3::new(a.cos(), 0.0, a.sin());
        let c = curtain_mesh(flat, 0.0, 1.0, 1.0);
        assert_eq!((c.positions.len(), c.indices.len()), ((CURTAIN_SEGMENTS + 1) * 3, CURTAIN_SEGMENTS * 2 * 6));
        assert!(c.colours.iter().skip(2).step_by(3).all(|c| c[3] == 0.0), "clear at the top");
        assert!(c.positions.iter().skip(2).step_by(3).all(|p| p[1] > 0.0), "standing up");
        let l = line_mesh(|a, o| flat(a) * (1.0 + o), Vec3::ONE, 1.0);
        assert_eq!(l.positions.len(), (LINE_SEGMENTS + 1) * 5);
        assert!(l.indices.iter().all(|&i| (i as usize) < l.positions.len()));
        let mut both = RingMesh::default();
        append(&mut both, c);
        append(&mut both, l);
        assert!(both.indices.iter().all(|&i| (i as usize) < both.positions.len()));
    }
}
