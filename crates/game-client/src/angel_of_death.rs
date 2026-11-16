//! The Angel of Death itself (`game_core::unit::UnitKind::AngelOfDeath`, `game_core::angel_of_death::Flight`): a
//! world view of each living one, the original "winged death" object-bank mesh rather than a 2D unit sprite
//! (pop3-rev-analysis.md "Angel of Death mechanics" "Visuals" and "Angel of Death flight pose, wing beat and dive").
//!
//! One textured mesh at a time, its pose's base object (`catalog::WINGED_DEATH_BEATING` while closing in and
//! carrying, `WINGED_DEATH_GLIDING` otherwise), its points replaced every drawn frame by the 12-frame loop of its
//! morph record (`pop3_format::morph`, key frames and half-way points). A pose change walks a 0..5 counter one step a
//! turn, the points snapping through the transition frames (`WINGED_DEATH_TRANSITION`) from the last loop frame,
//! then switches (`Wings`). Turned by its heading and tilted by the simulation's own pitch and bank, lifted to its
//! flight height, glided between ticks (`Track`, `[ours]`: whether the original interpolates is not traced). The
//! loop runs at 12 frames a second of game time (`[ours]`: the original's draw rate is not measured). With
//! `--no-original`, a stand-in: a body and two wings flapping with the same loop, swept back while gliding.

use crate::camera::CurveParamsRes;
use crate::game_frame::GameYaw;
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use game_core::angel_of_death::Flight;
use game_core::angle::{Angle, TURN};
use game_core::map::GameMap;
use game_core::time::{Tick, Ticks};
use game_core::tribes::{Owner, TribeId};
use game_core::unit::{torus_delta, Action, Unit, UnitId, UnitKind};
use pop3_format::morph::{morph_points, MorphFrame};
use pop3_format::{catalog, Atlas, Morph, Morphs, Object, ObjectBank, Theme, WORLD_UNITS_PER_CELL};

/// Turns of the original a second, and its loop frames a second (`[ours]`: one a drawn frame in the original, its
/// draw rate not measured): one clock for both.
const CLOCK_RATE: f32 = 12.0;
/// The transition counter's two ends: gliding, beating.
const GLIDING_END: u8 = 0;
const BEATING_END: u8 = 5;
/// Drawn frames a transition step's one-shot takes to land.
const ONE_SHOT_STEPS: u32 = 2;
/// Stand-in body (cells): wingspan of one wing, body length, thickness; flap and sweep amplitudes (radians).
const WING: f32 = 0.6;
const LENGTH: f32 = 0.55;
const DEPTH: f32 = 0.1;
const FLAP: f32 = 0.6;
const SWEEP: f32 = 0.7;

const DEFAULT_THEME: u8 = 0;

fn radians(steps: i32) -> f32 {
    steps as f32 * std::f32::consts::TAU / TURN as f32
}

/// Its two poses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Beating,
    Gliding,
}

impl Pose {
    /// The pose it asks for in `action`: beating from the moment it commits to a grab until it releases its victim.
    pub fn of(action: &Action) -> Pose {
        match action {
            Action::Closing { wait, .. } if wait.left() == Ticks::ZERO => Pose::Beating,
            Action::Carrying { .. } => Pose::Beating,
            _ => Pose::Gliding,
        }
    }

    fn end(self) -> u8 {
        match self {
            Pose::Beating => BEATING_END,
            Pose::Gliding => GLIDING_END,
        }
    }

    pub fn base(self) -> usize {
        match self {
            Pose::Beating => catalog::WINGED_DEATH_BEATING,
            Pose::Gliding => catalog::WINGED_DEATH_GLIDING,
        }
    }
}

/// A one-shot morph: from `from` to `to` over `ONE_SHOT_STEPS` frames from clock `start`, then held.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OneShot {
    from: usize,
    to: usize,
    start: f32,
}

/// Its wings: the pose drawn, the transition counter, when its loop restarted, the key frame it last showed and the
/// one-shot under way, all on the clock (`CLOCK_RATE` a second).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Wings {
    pub drawn: Pose,
    counter: u8,
    turns: i64,
    cycle_start: f32,
    current: usize,
    one_shot: Option<OneShot>,
}

impl Wings {
    /// Gliding, set at once.
    pub fn new(clock: f32) -> Self {
        Wings { drawn: Pose::Gliding, counter: GLIDING_END, turns: clock.floor() as i64, cycle_start: clock.floor(), current: catalog::WINGED_DEATH_GLIDING + 1, one_shot: None }
    }

    /// One turn at `clock` asking for `request`.
    pub fn step(&mut self, request: Pose, clock: f32) {
        let end = request.end();
        if self.counter == end {
            if self.drawn != request || self.one_shot.is_some() {
                (self.drawn, self.cycle_start, self.one_shot) = (request, clock, None);
            }
            return;
        }
        self.counter = if end > self.counter { self.counter + 1 } else { self.counter - 1 };
        let to = catalog::WINGED_DEATH_TRANSITION[self.counter as usize / 2];
        self.one_shot = Some(OneShot { from: self.current, to, start: clock });
    }

    /// Every turn from the last one stepped up to `clock`, asking for `request` (at most a second's worth).
    pub fn advance(&mut self, request: Pose, clock: f32) {
        let now = clock.floor() as i64;
        self.turns = self.turns.max(now - CLOCK_RATE as i64);
        while self.turns < now {
            self.turns += 1;
            self.step(request, self.turns as f32);
        }
    }

    /// The morph shown at `clock`: the one-shot under way, else the loop of `morph`, remembering its key frame.
    pub fn shown(&mut self, morph: &Morph, clock: f32) -> Option<MorphFrame> {
        if let Some(o) = self.one_shot {
            let t = ((clock - o.start).max(0.0) as u32).min(ONE_SHOT_STEPS);
            return Some(MorphFrame { from: o.from, to: o.to, num: t, den: ONE_SHOT_STEPS });
        }
        let frame = morph.frame((clock - self.cycle_start).max(0.0) as u32)?;
        self.current = frame.from;
        Some(frame)
    }

    /// The loop frame (0..`len`) at `clock`, None during a one-shot.
    pub fn loop_frame(&self, clock: f32, len: u32) -> Option<u32> {
        (self.one_shot.is_none() && len > 0).then(|| (clock - self.cycle_start).max(0.0) as u32 % len)
    }

    /// How far into gliding (1) from beating (0) it is: the counter's share.
    pub fn swept(&self) -> f32 {
        1.0 - self.counter as f32 / BEATING_END as f32
    }
}

/// `frame` of the bank's objects as a whole object: the base's faces, the points morphed.
pub fn posed(bank: &ObjectBank, base: usize, frame: MorphFrame) -> Option<Object> {
    let (base, from, to) = (bank.get(base)?, bank.objects.get(frame.from)?, bank.objects.get(frame.to)?);
    if from.points.len() != base.points.len() || to.points.len() != base.points.len() {
        return None;
    }
    Some(Object { points: morph_points(&from.points, &to.points, frame.num, frame.den), faces: base.faces.clone() })
}

/// The stand-in's wing angles (flap up, sweep back), radians: flapping with the loop while beating, swept back by
/// how far it is into gliding.
pub fn stand_in_wings(loop_frame: Option<u32>, swept: f32) -> (f32, f32) {
    let flap = loop_frame.map_or(0.0, |f| (f as f32 / CLOCK_RATE * std::f32::consts::TAU).sin() * FLAP * (1.0 - swept));
    (flap, SWEEP * swept)
}

/// What the simulation last had it at, to glide from: place, height, heading, pitch and bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    at: (u16, u16),
    flight: Flight,
}

impl Snap {
    fn of(u: &Unit) -> Snap {
        Snap { at: (u.x, u.z), flight: u.angel.map_or(Flight::default(), |a| a.flight) }
    }
}

/// The two last snaps and the map tick of the newer one.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Track {
    prev: Snap,
    cur: Snap,
    at: Tick,
}

impl Track {
    fn new(s: Snap, at: Tick) -> Self {
        Track { prev: s, cur: s, at }
    }

    /// Takes the map's state at tick `now`, shifting the last one back once a tick.
    fn see(&mut self, s: Snap, now: Tick) {
        if now != self.at {
            (self.prev, self.at) = (self.cur, now);
        }
        self.cur = s;
    }

    /// Where it is drawn at `drawn`: from the previous snap to the newer one over the tick before it.
    fn glide(&self, drawn: SimNow) -> (Vec2, Flight, f32) {
        let t = (drawn.tick.since(self.at) as f32 + 1.0 + drawn.frac).clamp(0.0, 1.0);
        glide_snap(self.prev, self.cur, t)
    }
}

/// `t` of the way from `a` to `b`: place in cells (at once on a jump over a cell), the flight's height and tilt, and
/// the heading (steps, fractional) the short way round.
pub fn glide_snap(a: Snap, b: Snap, t: f32) -> (Vec2, Flight, f32) {
    let at = crate::units::glide(a.at, b.at, t);
    let lerp = |x: i32, y: i32| (x as f32 + (y - x) as f32 * t).round() as i32;
    let heading = a.flight.heading.signed() as f32 + Angle::new(b.flight.heading.steps().wrapping_sub(a.flight.heading.steps()) % TURN).signed() as f32 * t;
    let flight = Flight { height: lerp(a.flight.height, b.flight.height), pitch: lerp(a.flight.pitch, b.flight.pitch), bank: lerp(a.flight.bank, b.flight.bank), ..b.flight };
    (at, flight, heading)
}

/// Whether two places are more than a cell apart along an axis.
fn far(a: (u16, u16), b: (u16, u16)) -> bool {
    torus_delta(a.0, b.0).unsigned_abs() > WORLD_UNITS_PER_CELL || torus_delta(a.1, b.1).unsigned_abs() > WORLD_UNITS_PER_CELL
}

/// How high its body is drawn over the ground at `at` (cells), render units.
fn lift(map: &GameMap, at: Vec2, height: i32, height_scale: f32) -> f32 {
    let w = |c: f32| (c * WORLD_UNITS_PER_CELL as f32).rem_euclid(65536.0) as u32;
    let ground = map.terrain.height_at(w(at.x), w(at.y), WORLD_UNITS_PER_CELL);
    (height - ground).max(0) as f32 * height_scale
}

/// Its body's own turn: yaw from its `heading` (steps, fractional), nose up by `pitch`, leaning by `bank` towards a
/// growing heading. Its nose is the mesh's −z.
pub fn body_rotation(heading: f32, pitch: i32, bank: i32) -> Quat {
    let yaw: Quat = GameYaw(std::f32::consts::FRAC_PI_2 - heading * std::f32::consts::TAU / TURN as f32).into();
    yaw * Quat::from_rotation_x(radians(pitch)) * Quat::from_rotation_z(radians(bank))
}

/// The morphs, the atlas and the per-tribe materials made so far (the atlas texture is theme-wide, like
/// `buildings`/`vehicles`).
#[derive(Resource, Default)]
pub struct AngelLook {
    theme: Option<u8>,
    atlas: Option<Handle<Image>>,
    morphs: Option<Option<Morphs>>,
    materials: Vec<(Owner, Handle<StandardMaterial>)>,
}

impl AngelLook {
    fn prepare(&mut self, theme: u8, levels: &LevelList, images: &mut Assets<Image>) {
        if self.theme != Some(theme) {
            self.theme = Some(theme);
            self.atlas = angel_atlas(levels, theme, images);
            self.materials.clear();
        }
        if self.morphs.is_none() {
            self.morphs = Some(levels.original.then(|| Morphs::load(&levels.objects_dir, 0).map_err(|e| warn!("angel of death morphs: {e}")).ok()).flatten());
        }
    }

    fn morph(&self, pose: Pose) -> Option<&Morph> {
        self.morphs.as_ref()?.as_ref()?.for_base(pose.base())
    }

    /// The material of `owner`: the theme's atlas (tribe colour baked into the mesh's own UVs) or a flat tint.
    fn material(&mut self, owner: Owner, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        if let Some((_, m)) = self.materials.iter().find(|(o, _)| *o == owner) {
            return m.clone();
        }
        let m = mats.add(match &self.atlas {
            Some(tex) => StandardMaterial { base_color_texture: Some(tex.clone()), perceptual_roughness: 0.95, cull_mode: Some(Face::Back), ..default() },
            None => StandardMaterial { base_color: crate::sites::tribe_color(owner), perceptual_roughness: 0.8, cull_mode: None, ..default() },
        });
        self.materials.push((owner, m.clone()));
        m
    }
}

/// The theme's atlas as a texture, None without original files.
fn angel_atlas(levels: &LevelList, theme: u8, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("angel of death atlas for theme {theme}: {e}")).ok()?;
    Some(images.add(atlas_image(&atlas, &palette.palette)))
}

/// The asset stores an angel's parts are made in.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Assets3d<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

/// Where an angel holds its victim, in its view's own space (`Mouth::at`, under the view's scale and turn): the
/// tip of its head, moved with its glided height, heading and tilt (`units::pull_to_eye` draws the victim there).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Mouth {
    pub of: UnitId,
    pub at: Vec3,
}

/// The tip of the head among `positions` (a posed body): the frontmost point (−z) near its centre line, so never a
/// wing tip swept forward.
pub fn head_tip(positions: &[[f32; 3]]) -> Vec3 {
    let span = positions.iter().map(|p| p[0].abs()).fold(0.0, f32::max);
    positions.iter().filter(|p| p[0].abs() <= span * 0.15).map(|&p| Vec3::from(p)).min_by(|a, b| a.z.total_cmp(&b.z)).unwrap_or(Vec3::ZERO)
}

/// What its body is drawn with: the original's morphed mesh, or the stand-in's two wing pivots.
enum Body {
    Original { mesh: Handle<Mesh>, tribe: TribeId },
    StandIn { wings: [Entity; 2] },
}

/// One angel's view: the root (moved over the ground), its hull (lifted and turned) and its body.
#[derive(Component)]
pub struct AngelOfDeathView {
    me: Entity,
    hull: Entity,
    body: Body,
    born: Tick,
}

impl AngelOfDeathView {
    fn clock(&self, now: SimNow) -> f32 {
        now.secs_since(self.born).max(0.0) * CLOCK_RATE
    }
}

/// The original's mesh for the gliding pose's first loop frame, None without the files.
fn first_mesh(objects: &OriginalObjects, look: &AngelLook, tribe: TribeId) -> Option<Mesh> {
    let bank = objects.0.as_ref()?;
    let frame = look.morph(Pose::Gliding)?.frame(0)?;
    Some(to_mesh(object_mesh(&solid_part(&posed(bank, Pose::Gliding.base(), frame)?), tribe)))
}

impl WorldView for AngelOfDeathView {
    type Model = Unit;
    type Params = (
        (Res<'static, CurrentMap>, Res<'static, CurveParamsRes>, Res<'static, LevelList>, Res<'static, OriginalObjects>),
        ResMut<'static, AngelLook>,
        Assets3d<'static>,
        Query<'static, 'static, (&'static mut Grounded, &'static mut Track, &'static mut Wings, &'static mut Mouth)>,
        Query<'static, 'static, &'static mut Transform, Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Unit> {
        map.units.iter().filter(|u| u.kind == UnitKind::AngelOfDeath)
    }

    fn id(u: &Unit) -> u32 {
        u.id.raw()
    }

    fn spawn(u: &Unit, view: &mut EntityCommands, ((map, params, levels, objects), look, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        look.prepare(map.0.theme.unwrap_or(DEFAULT_THEME), levels, &mut assets.images);
        let tribe = u.owner.tribe().unwrap_or(TribeId::Blue);
        let material = look.material(u.owner, &mut assets.materials);
        let snap = Snap::of(u);
        let me = view.id();
        let at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let pose = Transform::from_xyz(0.0, lift(&map.0, at, snap.flight.height, params.0.height_scale), 0.0).with_rotation(body_rotation(snap.flight.heading.signed() as f32, 0, 0));
        view.insert((Grounded { at, half: 0.0 }, Track::new(snap, map.0.now), Wings::new(0.0), Mouth { of: u.id, at: pose.transform_point(Vec3::new(0.0, 0.0, -LENGTH / 2.0)) }, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), crate::hover::Hoverable::default(), Transform::default(), Visibility::Hidden));
        let original = first_mesh(objects, look, tribe).map(|m| assets.meshes.add(m));
        let (mut hull, mut wings) = (Entity::PLACEHOLDER, [Entity::PLACEHOLDER; 2]);
        view.with_children(|c| {
            let mut h = c.spawn((pose, Visibility::Inherited));
            match &original {
                Some(mesh) => {
                    h.with_children(|h| {
                        h.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                    });
                }
                None => {
                    let (body, wing) = (assets.meshes.add(Cuboid::new(WING * 0.4, DEPTH, LENGTH)), assets.meshes.add(Cuboid::new(WING, DEPTH * 0.5, LENGTH * 0.6)));
                    h.with_children(|h| {
                        h.spawn((Mesh3d(body), MeshMaterial3d(material.clone())));
                        for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                            wings[k] = h
                                .spawn((Transform::from_xyz(side * WING * 0.2, 0.0, 0.0), Visibility::Inherited))
                                .with_children(|p| {
                                    p.spawn((Mesh3d(wing.clone()), MeshMaterial3d(material.clone()), Transform::from_xyz(side * WING * 0.5, 0.0, 0.0)));
                                })
                                .id();
                        }
                    });
                }
            }
            hull = h.id();
        });
        let body = match original {
            Some(mesh) => Body::Original { mesh, tribe },
            None => Body::StandIn { wings },
        };
        Some(AngelOfDeathView { me, hull, body, born: map.0.now })
    }

    fn update(&self, u: &Unit, now: SimNow, ((map, params, _, objects), look, assets, tracked, transforms): &mut SystemParamItem<Self::Params>) {
        let Ok((mut g, mut track, mut wings, mut mouth)) = tracked.get_mut(self.me) else { return };
        let snap = Snap::of(u);
        if far(track.cur.at, snap.at) {
            *track = Track::new(snap, map.0.now);
        }
        track.see(snap, map.0.now);
        let (at, flight, heading) = track.glide(now);
        g.at = at;
        let clock = self.clock(now);
        wings.advance(Pose::of(&u.action), clock);
        let mut tip = Vec3::new(0.0, 0.0, -LENGTH / 2.0);
        if let Body::Original { mesh, tribe } = &self.body {
            let frame = look.morph(wings.drawn).and_then(|m| wings.shown(m, clock));
            if let Some(obj) = objects.0.as_ref().zip(frame).and_then(|(bank, frame)| posed(bank, wings.drawn.base(), frame)) {
                let data = object_mesh(&solid_part(&obj), *tribe);
                tip = head_tip(&data.positions);
                assets.meshes.insert(mesh, to_mesh(data)).ok();
            }
        }
        if let Ok(mut t) = transforms.get_mut(self.hull) {
            t.translation.y = lift(&map.0, at, flight.height, params.0.height_scale);
            t.rotation = body_rotation(heading, flight.pitch, flight.bank);
            mouth.at = t.transform_point(tip);
        }
        match &self.body {
            Body::Original { .. } => {}
            Body::StandIn { wings: pivots } => {
                let (flap, sweep) = stand_in_wings(wings.loop_frame(clock, CLOCK_RATE as u32), wings.swept());
                for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                    if let Ok(mut t) = transforms.get_mut(pivots[k]) {
                        t.rotation = Quat::from_rotation_y(side * sweep) * Quat::from_rotation_z(side * flap);
                    }
                }
            }
        }
    }
}

pub struct AngelOfDeathPlugin;

impl Plugin for AngelOfDeathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AngelLook>().add_systems(Update, sync_views::<AngelOfDeathView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_tip_is_the_frontmost_point_on_its_centre_line_not_a_forward_wing_tip() {
        let points = [[0.0, 0.0, 0.3], [0.05, 0.1, -0.4], [-1.0, 0.0, -0.6], [1.0, 0.2, 0.1], [0.0, -0.1, -0.2]];
        assert_eq!(head_tip(&points), Vec3::new(0.05, 0.1, -0.4));
        assert_eq!(head_tip(&[]), Vec3::ZERO);
    }
    use game_core::time::Countdown;
    use game_core::unit::UnitId;

    fn obj(points: Vec<[i16; 3]>) -> Object {
        Object { points, faces: Vec::new() }
    }

    fn angel_morph() -> Morph {
        Morph { base: 100, keys: (101..=106).collect(), steps: vec![1; 6] }
    }

    #[test]
    fn it_beats_its_wings_from_the_moment_it_commits_until_it_lets_go() {
        let target = UnitId::from_raw(1);
        assert_eq!(Pose::of(&Action::Hunting), Pose::Gliding);
        assert_eq!(Pose::of(&Action::Closing { target, wait: Countdown::new(Ticks::new(2)) }), Pose::Gliding, "the standoff still glides");
        assert_eq!(Pose::of(&Action::Closing { target, wait: Countdown::new(Ticks::ZERO) }), Pose::Beating);
        assert_eq!(Pose::of(&Action::Carrying { victim: target, left: Countdown::new(Ticks::new(28)) }), Pose::Beating);
    }

    #[test]
    fn toward_beating_the_wings_snap_through_the_transition_frames_then_switch() {
        let mut w = Wings::new(0.0);
        let mut seen = Vec::new();
        for turn in 1..=6 {
            w.step(Pose::Beating, turn as f32);
            seen.push(w.one_shot.map(|o| o.to));
        }
        assert_eq!(seen, vec![Some(114), Some(115), Some(115), Some(116), Some(116), None]);
        assert_eq!(w.drawn, Pose::Beating, "switched on the 6th turn");
        assert_eq!(w.shown(&angel_morph(), 6.0), Some(MorphFrame { from: 101, to: 102, num: 0, den: 2 }), "its loop from frame 0");
    }

    #[test]
    fn toward_gliding_the_other_way_round_and_a_reversal_walks_back() {
        let mut w = Wings::new(0.0);
        (1..=6).for_each(|t| w.step(Pose::Beating, t as f32));
        let back: Vec<_> = (7..=12)
            .map(|t| {
                w.step(Pose::Gliding, t as f32);
                w.one_shot.map(|o| o.to)
            })
            .collect();
        assert_eq!(back, vec![Some(116), Some(115), Some(115), Some(114), Some(114), None]);
        assert_eq!(w.drawn, Pose::Gliding);
        let mut w = Wings::new(0.0);
        (1..=3).for_each(|t| w.step(Pose::Beating, t as f32));
        w.step(Pose::Gliding, 4.0);
        assert_eq!((w.counter, w.drawn), (2, Pose::Gliding), "walked back a step, never having switched");
    }

    #[test]
    fn a_one_shot_lands_in_two_frames_from_the_frozen_loop_frame_and_holds() {
        let mut w = Wings::new(0.0);
        let m = angel_morph();
        assert_eq!(w.shown(&m, 3.0).unwrap().from, 102, "loop frame 3: from key 102");
        w.step(Pose::Beating, 3.0);
        assert_eq!(w.shown(&m, 3.0), Some(MorphFrame { from: 102, to: 114, num: 0, den: 2 }));
        assert_eq!(w.shown(&m, 4.5).unwrap().num, 1);
        assert_eq!(w.shown(&m, 9.0).unwrap().num, 2, "held");
    }

    #[test]
    fn the_loop_wraps_every_twelve_frames() {
        let mut w = Wings::new(0.0);
        let m = angel_morph();
        assert_eq!(w.shown(&m, 0.0), w.shown(&m, 12.0));
        assert_eq!(w.loop_frame(13.5, 12), Some(1));
        w.step(Pose::Beating, 14.0);
        assert_eq!(w.loop_frame(14.0, 12), None, "not during a one-shot");
    }

    #[test]
    fn advancing_steps_every_turn_passed_once() {
        let mut w = Wings::new(0.0);
        w.advance(Pose::Beating, 2.7);
        assert_eq!(w.counter, 2);
        w.advance(Pose::Beating, 2.9);
        assert_eq!(w.counter, 2, "no new turn");
    }

    #[test]
    fn a_posed_frame_has_the_base_faces_and_points_between_two_keys() {
        let mut bank = ObjectBank { objects: vec![Object::default(); 117] };
        let face = pop3_format::Face { tile: Some(1), colour: 0, points: vec![0, 1, 2], uv: vec![(0, 0); 3], flags: 0 };
        bank.objects[100] = Object { points: vec![[0; 3]; 3], faces: vec![face.clone()] };
        bank.objects[101] = obj(vec![[0, 0, 0], [10, 0, 0], [0, 10, 0]]);
        bank.objects[102] = obj(vec![[0, 0, 0], [20, 0, 0], [0, 30, 0]]);
        let p = posed(&bank, 100, MorphFrame { from: 101, to: 102, num: 1, den: 2 }).unwrap();
        assert_eq!(p.points, vec![[0, 0, 0], [15, 0, 0], [0, 20, 0]]);
        assert_eq!(p.faces, vec![face]);
    }

    #[test]
    fn the_stand_in_flaps_while_beating_and_sweeps_back_gliding() {
        assert_eq!(stand_in_wings(Some(0), 0.0), (0.0, 0.0));
        assert!((stand_in_wings(Some(3), 0.0).0 - FLAP).abs() < 1e-5, "a quarter loop: wings up");
        assert_eq!(stand_in_wings(None, 1.0), (0.0, SWEEP));
    }

    fn snap(at: (u16, u16), heading: u32, height: i32) -> Snap {
        Snap { at, flight: Flight { heading: Angle::new(heading), height, ..Flight::default() } }
    }

    #[test]
    fn it_glides_between_ticks_the_short_way_round() {
        let (at, f, heading) = glide_snap(snap((512, 512), 2000, 100), snap((1024, 512), 40, 300), 0.5);
        assert_eq!(at, Vec2::new(1.5, 1.0));
        assert_eq!(f.height, 200);
        assert!((heading - -4.0).abs() < 1e-3, "across 0, not round the long way: {heading}");
    }

    #[test]
    fn a_track_glides_over_the_tick_before_its_newer_snap() {
        let mut t = Track::new(snap((0, 0), 0, 0), Tick::from_wire(10));
        t.see(snap((100, 0), 0, 0), Tick::from_wire(11));
        let (at, ..) = t.glide(SimNow { tick: Tick::from_wire(10), frac: 0.5 });
        assert_eq!(at.x, 50.0 / WORLD_UNITS_PER_CELL as f32);
        let (at, ..) = t.glide(SimNow { tick: Tick::from_wire(11), frac: 0.0 });
        assert_eq!(at.x, 100.0 / WORLD_UNITS_PER_CELL as f32);
    }

    #[test]
    fn lift_never_goes_below_the_ground() {
        let map = GameMap::sandbox_units();
        let ground = map.terrain.height_at(1024, 1024, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, Vec2::splat(2.0), ground + 600, 0.5), 300.0);
        assert_eq!(lift(&map, Vec2::splat(2.0), ground - 600, 0.5), 0.0);
    }

    /// A drawn direction back in the game's frame (x, up, z).
    fn game(r: Vec3) -> Vec3 {
        Vec3::new(r.x, r.y, -r.z)
    }

    #[test]
    fn the_body_turns_with_its_heading_and_tilts_nose_up_and_into_the_turn() {
        let nose = |h: f32, pitch: i32| game(body_rotation(h, pitch, 0) * Vec3::NEG_Z);
        assert!((nose(0.0, 0) - Vec3::X).length() < 1e-5, "heading 0 is +x");
        assert!((nose(512.0, 0) - Vec3::Z).length() < 1e-5, "a quarter turn is +z");
        assert!(nose(0.0, 256).y > 0.5, "pitch: nose up");
        let toward_turn = |bank: i32| game(body_rotation(0.0, 0, bank) * body_side());
        assert!(toward_turn(256).y < -0.5, "the wing on the side it turns to dips: {}", toward_turn(256));
    }

    /// The local direction that points to game +z (where a growing heading turns) at heading 0.
    fn body_side() -> Vec3 {
        let r = body_rotation(0.0, 0, 0);
        let want = Vec3::from(crate::game_frame::GamePos(Vec3::Z));
        r.inverse() * want
    }
}
