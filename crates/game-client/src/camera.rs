//! Orbit camera around a wrapping focus point. Pushing the mouse against the window
//! border, Up-Down or WASD move, Left-Right and middle-drag rotate, Enter toggles the aerial view.
//! The mouse is captured by the in-game cursor (`virtual_cursor`, Esc releases it).

use crate::edge_push::EdgePush;
use crate::terrain_mesh::{focus_height, CurveParams};
use crate::world::CurrentMap;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

const MAP: f32 = pop3_format::MAP_SIZE as f32;
/// Default ground view tilt in degrees and distance in cells: low and close, like the original.
pub const GROUND_PITCH_DEG: f32 = 6.0;
pub const GROUND_DISTANCE: f32 = 14.0;
/// (pitch radians, distance) of the default ground view.
pub const GROUND_VIEW: (f32, f32) = (GROUND_PITCH_DEG * std::f32::consts::PI / 180.0, GROUND_DISTANCE);
/// Scroll speeds in camera-distances per second (mouse = at full push).
pub const MOUSE_SPEED: f32 = 2.2;
pub const KEY_SPEED: f32 = 3.0;
/// Tilt in radians/s and zoom as fraction of distance per second.
pub const TILT_SPEED: f32 = 0.6;
pub const ZOOM_SPEED: f32 = 1.2;
pub const DEFAULT_FOV: f32 = std::f32::consts::FRAC_PI_3;
pub const FOV_RANGE: (f32, f32) = (0.3, 2.2);
pub const FOV_SPEED: f32 = 0.6;
pub const PITCH_RANGE: (f32, f32) = (0.05, 1.5);
pub const DISTANCE_RANGE: (f32, f32) = (3.0, 220.0);
/// The eye never goes lower than the map's highest ground plus this (cells), like the original's
/// fixed camera elevation: low ground near the sea cannot hide it behind a cliff.
pub const EYE_CLEARANCE: f32 = 1.5;
/// Seconds a flight to a place (H, Space) takes.
pub const FLIGHT_SECS: f32 = 0.4;
const SKY: Color = Color::srgb(0.45, 0.65, 0.92);
const SPACE: Color = Color::srgb(0.02, 0.02, 0.06);

#[derive(Resource, Clone, Debug)]
pub struct CameraRig {
    /// Focus in cell coordinates, always kept in `[0, MAP)`.
    pub focus: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub aerial: bool,
    /// Vertical field of view in radians (lens width: wider = stronger perspective).
    pub fov: f32,
    /// Ground-level settings restored when leaving the aerial view.
    saved: (f32, f32),
    /// A quick flight of the focus to a place under way (`fly_to`).
    flight: Option<Flight>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Flight {
    from: Vec2,
    to: Vec2,
    elapsed: f32,
}

/// Focus `t` (0..1) of the way from `from` to `to`, the short way around the torus, easing in and
/// out (smoothstep).
pub fn flight_point(from: Vec2, to: Vec2, t: f32) -> Vec2 {
    let t = t.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    let wrap = |d: f32| (d + MAP / 2.0).rem_euclid(MAP) - MAP / 2.0;
    let delta = Vec2::new(wrap(to.x - from.x), wrap(to.y - from.y));
    (from + delta * eased).rem_euclid(Vec2::splat(MAP))
}

impl Default for CameraRig {
    fn default() -> Self {
        let view = ground_view(env_var);
        CameraRig {
            focus: Vec2::splat(MAP / 2.0),
            yaw: 0.0,
            pitch: view.0,
            distance: view.1,
            aerial: false,
            fov: DEFAULT_FOV,
            saved: view,
            flight: None,
        }
    }
}

impl CameraRig {
    /// Ground-plane forward (where "up arrow" goes) for the current yaw.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(-self.yaw.sin(), -self.yaw.cos())
    }

    /// Moving by hand cancels a flight under way.
    pub fn move_by(&mut self, forward: f32, right: f32) {
        if forward != 0.0 || right != 0.0 {
            self.flight = None;
        }
        let f = self.forward();
        let r = Vec2::new(-f.y, f.x);
        self.focus = (self.focus + f * forward + r * right).rem_euclid(Vec2::splat(MAP));
    }

    pub fn toggle_aerial(&mut self) {
        self.aerial = !self.aerial;
        if self.aerial {
            self.saved = (self.pitch, self.distance);
            (self.pitch, self.distance) = (1.35, 115.0);
        } else {
            (self.pitch, self.distance) = self.saved;
        }
    }

    /// New ground view (leaves the aerial view): pitch in radians, distance in cells.
    pub fn set_ground_view(&mut self, pitch: f32, distance: f32) {
        self.aerial = false;
        self.pitch = pitch.clamp(PITCH_RANGE.0, PITCH_RANGE.1);
        self.distance = distance.clamp(DISTANCE_RANGE.0, DISTANCE_RANGE.1);
        self.saved = (self.pitch, self.distance);
    }

    /// Tilt (radians, positive = look more downward) and zoom (positive = closer, fraction).
    pub fn adjust_view(&mut self, tilt: f32, zoom: f32) {
        self.pitch = (self.pitch + tilt).clamp(PITCH_RANGE.0, PITCH_RANGE.1);
        self.distance = (self.distance * (1.0 - zoom)).clamp(DISTANCE_RANGE.0, DISTANCE_RANGE.1);
    }

    /// Flies the focus to `to` (cells) in `FLIGHT_SECS`, instead of jumping there.
    pub fn fly_to(&mut self, to: Vec2) {
        self.flight = Some(Flight { from: self.focus, to: to.rem_euclid(Vec2::splat(MAP)), elapsed: 0.0 });
    }

    /// Moves a flight under way on by `dt` seconds.
    pub fn advance_flight(&mut self, dt: f32) {
        let Some(mut f) = self.flight else { return };
        f.elapsed += dt;
        self.focus = flight_point(f.from, f.to, f.elapsed / FLIGHT_SECS);
        self.flight = (f.elapsed < FLIGHT_SECS).then_some(f);
    }

    pub fn look_at_cell(&mut self, cell: (i32, i32)) {
        self.focus = Vec2::new(cell.0 as f32, cell.1 as f32).rem_euclid(Vec2::splat(MAP));
    }

    /// Eye offset from the focus point in render space.
    pub fn eye_offset(&self) -> Vec3 {
        let horiz = self.pitch.cos() * self.distance;
        Vec3::new(self.yaw.sin() * horiz, self.pitch.sin() * self.distance, self.yaw.cos() * horiz)
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(PreStartup, spawn_camera)
            .add_systems(Update, frame_new_map.run_if(resource_changed::<crate::world::LevelList>))
            .add_systems(Update, ((camera_input, fly).in_set(crate::menu::Gameplay), apply_rig, sky_color).chain());
    }
}

/// Draws the world and the HUD; inactive behind the main menu.
#[derive(Component)]
pub struct GameCamera;

/// Drawn after the game camera, without clearing it: the menus and the cursor. It clears the
/// window itself while the game camera is off.
#[derive(Component)]
pub struct OverlayCamera;

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn((GameCamera, Camera3d::default(), IsDefaultUiCamera, Transform::default()));
    commands.spawn((OverlayCamera, Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }));
    commands.insert_resource(ClearColor(SKY));
    commands.insert_resource(GlobalAmbientLight { brightness: 60.0, ..default() });
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, ..default() },
        Transform::from_xyz(40.0, 22.0, 15.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn frame_new_map(map: Res<CurrentMap>, mut rig: ResMut<CameraRig>) {
    let (cell, yaw) = start_view(&map.0);
    rig.look_at_cell(cell);
    rig.yaw = yaw;
}

/// The player's (tribe 0) reincarnation site, else some low inland ground.
pub fn start_cell(map: &game_core::map::GameMap) -> (i32, i32) {
    map.site_of(0).map_or_else(|| map.terrain.lowland_cell(), |s| s.cell())
}

/// Where a new map's camera looks and its yaw: the level's own start camera where it gives one (its
/// angle read like the things' angles, `nature::angle_yaw`), else `start_cell` facing yaw 0.
pub fn start_view(map: &game_core::map::GameMap) -> ((i32, i32), f32) {
    let camera = map.start_camera;
    let cell = camera.and_then(|c| c.cell).unwrap_or_else(|| start_cell(map));
    (cell, camera.map_or(0.0, |c| crate::nature::angle_yaw(c.angle)))
}

fn fly(time: Res<Time>, mut rig: ResMut<CameraRig>) {
    if rig.flight.is_some() {
        rig.advance_flight(time.delta_secs());
    }
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window>,
    cursor: Res<crate::virtual_cursor::VirtualCursor>,
    mut push: Local<EdgePush>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
) {
    let dt = time.delta_secs();
    let axis = |a: KeyCode, b: KeyCode| keys.pressed(a) as i32 as f32 - keys.pressed(b) as i32 as f32;
    rig.yaw += axis(KeyCode::ArrowLeft, KeyCode::ArrowRight) * 1.8 * dt;
    let pressed_any = |ks: &[KeyCode]| keys.any_pressed(ks.iter().copied()) as i32 as f32;
    let tilt = pressed_any(&[KeyCode::Home]) - pressed_any(&[KeyCode::End]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let page = pressed_any(&[KeyCode::PageUp]) - pressed_any(&[KeyCode::PageDown]);
    let zoom = if ctrl { page } else { 0.0 };
    rig.adjust_view(tilt * TILT_SPEED * dt, zoom * ZOOM_SPEED * dt);
    let widen = if shift && !ctrl { page } else { 0.0 };
    rig.fov = (rig.fov + widen * FOV_SPEED * dt).clamp(FOV_RANGE.0, FOV_RANGE.1);

    let edge = windows
        .iter()
        .next()
        .map_or(Vec2::ZERO, |w| push.update(cursor.effective(w.cursor_position()), w.size(), motion.delta, dt));
    let keys_move = key_move(|k| keys.pressed(k)) * KEY_SPEED;
    let speed = (edge * MOUSE_SPEED + keys_move).clamp(Vec2::splat(-KEY_SPEED), Vec2::splat(KEY_SPEED));
    let scale = rig.distance.max(10.0) * dt;
    rig.move_by(speed.x * scale, speed.y * scale);

    if mouse.pressed(MouseButton::Middle) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.005).clamp(PITCH_RANGE.0, PITCH_RANGE.1);
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        rig.toggle_aerial();
    }
}

/// Keyboard scroll as (forward, right) in -1..1: Up/Down or W/S, A/D strafe.
pub fn key_move(pressed: impl Fn(KeyCode) -> bool) -> Vec2 {
    let axis = |plus: &[KeyCode], minus: &[KeyCode]| {
        plus.iter().any(|&k| pressed(k)) as i32 as f32 - minus.iter().any(|&k| pressed(k)) as i32 as f32
    };
    Vec2::new(
        axis(&[KeyCode::ArrowUp, KeyCode::KeyW], &[KeyCode::ArrowDown, KeyCode::KeyS]),
        axis(&[KeyCode::KeyD], &[KeyCode::KeyA]),
    )
}

/// Where the eye goes: `offset` from the target, but never lower than `top` (the map's highest
/// ground, render units) plus `EYE_CLEARANCE`; it keeps looking at the target.
pub fn eye_position(target: Vec3, offset: Vec3, top: f32) -> Vec3 {
    let eye = target + offset;
    Vec3::new(eye.x, eye.y.max(top + EYE_CLEARANCE), eye.z)
}

fn apply_rig(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    mut top: Local<Option<(u32, u16)>>,
) {
    let terrain = &map.0.terrain;
    if top.is_none_or(|(revision, _)| revision != terrain.revision()) {
        *top = Some((terrain.revision(), terrain.heights().iter().copied().max().unwrap_or(0)));
    }
    let top = top.map_or(0, |t| t.1) as f32 * params.0.height_scale;
    let target = Vec3::Y * focus_height(terrain, (rig.focus.x, rig.focus.y), &params.0);
    for (mut t, mut projection) in &mut cam {
        if let Projection::Perspective(p) = projection.as_mut() && p.fov != rig.fov {
            p.fov = rig.fov;
        }
        let wanted = Transform::from_translation(eye_position(target, rig.eye_offset(), top)).looking_at(target, Vec3::Y);
        *t = Transform {
            translation: t.translation.lerp(wanted.translation, 0.25),
            rotation: t.rotation.slerp(wanted.rotation, 0.25),
            ..wanted
        };
    }
}

/// How far the view has gone from the sky to space (0 near the ground, 1 looking at the planet).
pub fn space_fade(distance: f32) -> f32 {
    ((distance - 30.0) / 60.0).clamp(0.0, 1.0)
}

/// Blue sky near the ground, black space when zoomed out to the planet.
pub fn sky_for_distance(distance: f32) -> Color {
    SKY.mix(&SPACE, space_fade(distance))
}

fn sky_color(rig: Res<CameraRig>, mut clear: ResMut<ClearColor>) {
    clear.0 = sky_for_distance(rig.distance);
}

#[derive(Resource)]
pub struct CurveParamsRes(pub CurveParams);

impl Default for CurveParamsRes {
    fn default() -> Self {
        CurveParamsRes(curve_overrides(CurveParams::default(), env_var))
    }
}

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn parsed(get: &impl Fn(&str) -> Option<String>, key: &str) -> Option<f32> {
    get(key).and_then(|v| v.parse().ok())
}

/// Dev tuning: `POP3_RELIEF` sets the relief (x original height ratio), `POP3_CURVATURE` the bend.
pub fn curve_overrides(mut p: CurveParams, get: impl Fn(&str) -> Option<String>) -> CurveParams {
    if let Some(relief) = parsed(&get, "POP3_RELIEF") {
        p = p.with_relief(relief);
    }
    p.curvature = parsed(&get, "POP3_CURVATURE").unwrap_or(p.curvature);
    p
}

/// Ground view (pitch rad, distance cells), `POP3_VIEW_PITCH` (degrees) / `POP3_VIEW_DISTANCE` override.
pub fn ground_view(get: impl Fn(&str) -> Option<String>) -> (f32, f32) {
    (
        parsed(&get, "POP3_VIEW_PITCH").map_or(GROUND_VIEW.0, f32::to_radians),
        parsed(&get, "POP3_VIEW_DISTANCE").unwrap_or(GROUND_VIEW.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_ground_view_leaves_aerial() {
        let mut rig = CameraRig::default();
        rig.toggle_aerial();
        rig.set_ground_view(0.2, 10.0);
        assert!(!rig.aerial);
        assert_eq!((rig.pitch, rig.distance), (0.2, 10.0));
        rig.toggle_aerial();
        rig.toggle_aerial();
        assert_eq!((rig.pitch, rig.distance), (0.2, 10.0), "aerial round trip restores it");
    }

    #[test]
    fn view_tuning_overrides() {
        let get = |k: &str| match k {
            "POP3_RELIEF" => Some("3".to_string()),
            "POP3_VIEW_PITCH" => Some("90".to_string()),
            "POP3_CURVATURE" => Some("oops".to_string()),
            _ => None,
        };
        let base = CurveParams::default();
        let p = curve_overrides(base, get);
        assert_eq!((p.relief(), p.curvature), (3.0, base.curvature), "relief from the env, bad curvature ignored");
        let (pitch, distance) = ground_view(get);
        assert!((pitch - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(distance, GROUND_VIEW.1);
        assert_eq!(ground_view(|_| None), GROUND_VIEW);
    }

    #[test]
    fn flights_ease_the_short_way_and_land_on_time() {
        let (from, to) = (Vec2::new(120.0, 10.0), Vec2::new(4.0, 10.0));
        assert_eq!(flight_point(from, to, 0.0), from);
        assert_eq!(flight_point(from, to, 1.0), to);
        assert_eq!(flight_point(from, to, 0.5), Vec2::new(126.0, 10.0), "12 cells across the map edge, not back across the map");
        assert!(flight_point(from, to, 0.1).x - from.x < 0.1 * 12.0, "starts slow");
        let mut rig = CameraRig::default();
        rig.fly_to(Vec2::new(20.0, 30.0));
        rig.advance_flight(FLIGHT_SECS / 2.0);
        assert!(rig.focus != Vec2::new(20.0, 30.0) && rig.flight.is_some(), "on the way");
        rig.advance_flight(FLIGHT_SECS);
        assert_eq!((rig.focus, rig.flight), (Vec2::new(20.0, 30.0), None), "landed");
        rig.fly_to(Vec2::new(50.0, 50.0));
        rig.move_by(1.0, 0.0);
        assert_eq!(rig.flight, None, "pushing the camera cancels the flight");
    }

    #[test]
    fn eye_stays_above_the_highest_ground() {
        let offset = Vec3::new(0.0, 1.5, 14.0);
        let low = eye_position(Vec3::ZERO, offset, 6.0);
        assert_eq!(low, Vec3::new(0.0, 6.0 + EYE_CLEARANCE, 14.0), "by the sea: up at the fixed elevation");
        let high = eye_position(Vec3::Y * 6.0, offset, 6.0);
        assert_eq!(high, Vec3::new(0.0, 7.5, 14.0), "on the highest ground: the usual offset");
        assert_eq!(eye_position(Vec3::ZERO, Vec3::new(0.0, 100.0, 5.0), 6.0).y, 100.0, "aerial: unchanged");
    }

    #[test]
    fn starts_on_player_site() {
        let mut map = game_core::map::GameMap::generate(3);
        map.sites = vec![game_core::site::ReincarnationSite::at_cell(0, (9, 99))];
        assert_eq!(start_cell(&map), (9, 99));
        map.sites.clear();
        assert_eq!(start_cell(&map), map.terrain.lowland_cell());
    }
    #[test]
    fn a_level_starts_its_camera_where_and_as_its_header_says() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        let site = start_cell(&map);
        assert_eq!(start_view(&map), (site, 0.0));
        map.start_camera = Some(game_core::map::StartCamera { cell: Some((21, 83)), angle: 512 });
        let (cell, yaw) = start_view(&map);
        assert_eq!(cell, (21, 83));
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "a quarter turn");
        map.start_camera = Some(game_core::map::StartCamera { cell: None, angle: 1024 });
        assert_eq!(start_view(&map).0, site, "an angle only: still the site");
    }


    #[test]
    fn wasd_moves_like_arrows_and_strafes() {
        let only = |ks: &'static [KeyCode]| move |k| ks.contains(&k);
        assert_eq!(key_move(only(&[KeyCode::KeyW])), Vec2::new(1.0, 0.0));
        assert_eq!(key_move(only(&[KeyCode::KeyW, KeyCode::ArrowUp])), Vec2::new(1.0, 0.0), "no double speed");
        assert_eq!(key_move(only(&[KeyCode::KeyS, KeyCode::KeyA])), Vec2::new(-1.0, -1.0));
        assert_eq!(key_move(only(&[KeyCode::KeyD])), Vec2::new(0.0, 1.0));
        assert_eq!(key_move(only(&[KeyCode::KeyW, KeyCode::ArrowDown])), Vec2::ZERO);
    }

    #[test]
    fn focus_wraps_around_the_torus() {
        let mut rig = CameraRig { focus: Vec2::new(1.0, 127.5), ..default() };
        rig.move_by(2.0, 0.0);
        assert!((rig.focus.y - 125.5).abs() < 1e-4);
        rig.move_by(-5.0, 0.0);
        assert!((rig.focus.y - 2.5).abs() < 1e-4);
    }

    #[test]
    fn adjust_view_tilts_zooms_and_clamps() {
        let mut rig = CameraRig::default();
        rig.adjust_view(0.1, 0.5);
        assert!((rig.pitch - (GROUND_VIEW.0 + 0.1)).abs() < 1e-6);
        assert!((rig.distance - GROUND_VIEW.1 * 0.5).abs() < 1e-6);
        rig.adjust_view(10.0, 0.99);
        assert_eq!((rig.pitch, rig.distance), (PITCH_RANGE.1, DISTANCE_RANGE.0));
    }

    #[test]
    fn sky_fades_to_space() {
        assert_eq!(sky_for_distance(GROUND_VIEW.1), SKY);
        assert_eq!(sky_for_distance(200.0), SPACE);
    }

    #[test]
    fn aerial_toggle_restores_ground_view() {
        let mut rig = CameraRig { distance: 42.0, ..default() };
        rig.toggle_aerial();
        assert!(rig.aerial && rig.distance > 100.0);
        rig.toggle_aerial();
        assert_eq!(rig.distance, 42.0);
    }
}
