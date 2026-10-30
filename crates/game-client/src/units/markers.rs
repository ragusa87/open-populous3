//! Markers over people, read from the unit's own state (docs/specs/pop3-rev-analysis.md "Status markers over
//! people"): low health sparkles (`Health::is_low`), shield rings, bloodlust bubbles and the hypnosis halo
//! (`Unit::statuses`), and the arrow over the player's selected people. Each view of a unit holds one slot per
//! marker (`SLOTS`), posed every frame at the drawn sim tick and raised a multiple of its body frame's height.
//! With the original files: their sprites, the arrow instead of the health bar, the low-health sparkles. Without:
//! generated stand-ins for shield, bloodlust and hypnosis only (selection keeps its health bar).

use super::art::{alpha_picture_frame, picture_frame, Frame};
use super::selection::Selection;
use super::{hidden_inside, upload_frames, FrameAsset, UnitView};
use crate::flame;
use crate::original_models::to_mesh;
use crate::sim_time::SimNow;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use game_core::time::Tick;
use game_core::unit::{Action, Unit};
use pop3_format::blend::AlphaTable;
use pop3_format::catalog::{BLOODLUST_SPRITES, EFFECT_SPRITE_FILE, HYPNOSIS_SPRITES, LOW_HEALTH_SPRITES, SELECTED_ARROW_SPRITE, SHIELD_SPRITES, TRIBES};
use pop3_format::{Picture, SpriteBank, Theme};
use std::f32::consts::TAU;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    LowHealth,
    Bloodlust,
    Hypnotised,
    Selected,
    /// Shield ring 0 (bright), 1 or 2 (faint), each one step behind the previous.
    Shield(u8),
    /// A small flame over a burning person (`Unit::catch_fire`, pop3-rev-analysis.md "Volcano lava flow"): no
    /// original sprite identified for it, so this always uses the generated flame (`flame::generated_tile`),
    /// unlike the other markers' original-art-when-available choice.
    Burning,
}

/// One slot per marker in each unit's view.
pub const SLOTS: [Marker; 8] = [Marker::LowHealth, Marker::Bloodlust, Marker::Hypnotised, Marker::Selected, Marker::Shield(0), Marker::Shield(1), Marker::Shield(2), Marker::Burning];

/// A shield ring's path ("Status markers over people"): its size (0-4) and height in eighths of the body, rising
/// from the feet to the head over 8 steps and back, largest at mid-height.
/// The burning flame's own size (cells), like the camp fire's own (`campfire::SIZE`) but a bit smaller.
const BURN_SIZE: f32 = 0.25;

pub const SHIELD_PATH: [(usize, u8); 16] =
    [(0, 0), (1, 1), (2, 2), (3, 3), (4, 4), (3, 5), (2, 6), (1, 7), (0, 8), (1, 7), (2, 6), (3, 5), (4, 4), (3, 3), (2, 2), (1, 1)];

/// A marker slot of a unit's view; `above`: how high its centre is over the feet (render units, unscaled).
#[derive(Component)]
pub struct MarkerSlot {
    pub slot: usize,
    pub above: f32,
}

/// The height (render units) of the body frame drawn now, on the unit's sprite: markers stand over it.
#[derive(Component, Default)]
pub struct BodyHeight(pub f32);

/// Whether `u` carries marker `m` at `now`; `selected` by the player, `original`: drawn with the original's art.
/// Shield and bloodlust blink in their last steps (`Timed::shown`, phased by the unit's id).
pub fn shown(m: Marker, u: &Unit, player: u8, selected: bool, original: bool, now: Tick) -> bool {
    u.is_alive()
        && match m {
            Marker::LowHealth => original && u.health.is_low(),
            Marker::Bloodlust => u.statuses.bloodlust.shown(now, u.id.raw()),
            Marker::Hypnotised => u.statuses.hypnotized.is_on(),
            Marker::Selected => original && selected && u.owner == player,
            Marker::Shield(_) => u.statuses.shield.shown(now, u.id.raw()),
            Marker::Burning => matches!(u.action, Action::OnFire { .. }),
        }
}

/// Marker `m` at sim tick `n` (`phase`: the person's own offset, for the shield): its frame in its art and the
/// height of its centre over the feet, in body heights.
pub fn pose(m: Marker, n: u32, phase: u32) -> (usize, f32) {
    match m {
        Marker::LowHealth | Marker::Bloodlust => ((n % 10) as usize, 1.11),
        Marker::Hypnotised => ((n % 11) as usize, 1.25),
        Marker::Selected => (0, 1.0),
        Marker::Shield(k) => {
            let i = ((n.wrapping_add(phase) & 15).wrapping_sub(k as u32) & 15) as usize;
            let (size, eighths) = SHIELD_PATH[i];
            (size + 5 * k as usize, (1.0 + eighths as f32 * 7.0 / 8.0) / 8.0)
        }
        // One new frame a tick: close enough to flame::FPS's own pacing for a secondary status cue.
        Marker::Burning => ((n as usize + phase as usize) % flame::FRAMES, 0.55),
    }
}

/// The frames of every marker, uploaded once.
#[derive(Resource)]
pub struct MarkerArt {
    /// The original's sprites: the arrow and the low-health sparkles are drawn, the health bar is not.
    pub original: bool,
    low_health: Vec<FrameAsset>,
    bloodlust: Vec<FrameAsset>,
    hypnosis: Vec<FrameAsset>,
    arrow: Vec<FrameAsset>,
    shield: Vec<FrameAsset>,
    /// The shield frames' materials tinted in each tribe's colour.
    shield_tribes: Vec<Vec<Handle<StandardMaterial>>>,
    burning: Vec<FrameAsset>,
}

impl MarkerArt {
    /// The mesh and material of `m`'s frame `i` over a person of tribe `owner`.
    fn frame(&self, m: Marker, i: usize, owner: u8) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        let frames = match m {
            Marker::LowHealth => &self.low_health,
            Marker::Bloodlust => &self.bloodlust,
            Marker::Hypnotised => &self.hypnosis,
            Marker::Selected => &self.arrow,
            Marker::Shield(_) => &self.shield,
            Marker::Burning => &self.burning,
        };
        let f = frames.get(i)?;
        let material = match m {
            Marker::Shield(_) => self.shield_tribes.get(owner as usize).and_then(|t| t.get(i)).cloned().unwrap_or_else(|| f.material.clone()),
            _ => f.material.clone(),
        };
        Some((f.mesh.clone(), material))
    }
}

/// Raw frames of each marker: low health, bloodlust, hypnosis, arrow, shield.
type Frames = [Vec<Frame>; 5];

fn original_frames(data_dir: &Path) -> Result<Frames, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let alpha = AlphaTable::load(data_dir, 0).map_err(err)?.rgba(&palette);
    let picture = |i: usize| {
        let sprite = bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"))?;
        Ok::<_, String>(Picture { sprite: sprite.clone(), origin: (sprite.width / 2, sprite.height / 2) })
    };
    let blended = |range: std::ops::RangeInclusive<usize>| range.map(|i| picture(i).map(|p| alpha_picture_frame(&p, &alpha))).collect::<Result<Vec<_>, _>>();
    Ok([
        blended(LOW_HEALTH_SPRITES)?,
        blended(BLOODLUST_SPRITES)?,
        blended(HYPNOSIS_SPRITES)?,
        vec![picture_frame(&picture(SELECTED_ARROW_SPRITE)?, &palette)],
        blended(SHIELD_SPRITES)?,
    ])
}

/// A soft dot of `colour` (alpha at its centre) at `(x, y)`, radius `r` (pixels), kept where already brighter.
fn dot(f: &mut Frame, (x, y): (f32, f32), r: f32, colour: [u8; 4]) {
    for py in 0..f.height {
        for px in 0..f.width {
            let d = ((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt();
            let a = (colour[3] as f32 * (1.0 - d / r).clamp(0.0, 1.0)) as u8;
            let i = (py * f.width + px) * 4;
            if a > f.rgba[i + 3] {
                f.rgba[i..i + 4].copy_from_slice(&[colour[0], colour[1], colour[2], a]);
            }
        }
    }
}

fn blank(width: usize, height: usize) -> Frame {
    Frame { width, height, origin: (width / 2, height / 2), rgba: vec![0; width * height * 4], scale: 1 }
}

/// A ring of `colour` in perspective, radii `rx`, `ry` round the frame's centre.
fn ring(f: &mut Frame, rx: f32, ry: f32, colour: [u8; 4]) {
    let c = (f.width as f32 / 2.0, f.height as f32 / 2.0);
    for k in 0..64 {
        let a = k as f32 * TAU / 64.0;
        dot(f, (c.0 + rx * a.cos(), c.1 + ry * a.sin()), 1.5, colour);
    }
}

/// Stand-in shield: rings 0 (bright), 1, 2 (faint), each in 5 sizes, cream so the tribe's tint shows.
pub fn generated_shield() -> Vec<Frame> {
    (0..3)
        .flat_map(|k| {
            (0..5).map(move |s| {
                let mut f = blank(32, 16);
                let rx = 7.0 + 2.0 * s as f32;
                ring(&mut f, rx, rx / 2.6, [255, 245, 225, [220, 140, 70][k]]);
                f
            })
        })
        .collect()
}

/// Stand-in bloodlust: three red bubbles rising and fading over 10 frames.
pub fn generated_bloodlust() -> Vec<Frame> {
    (0..10)
        .map(|n| {
            let mut f = blank(30, 25);
            for (b, x) in [8.0, 15.0, 22.0].into_iter().enumerate() {
                let p = ((n + 3 * b) % 10) as f32 / 10.0;
                dot(&mut f, (x, 21.0 - 17.0 * p), 3.0, [220, 30, 25, (230.0 * (1.0 - p)) as u8]);
            }
            f
        })
        .collect()
}

/// Stand-in hypnosis: a yellow ring with a bright spot going round it over 11 frames.
pub fn generated_hypnosis() -> Vec<Frame> {
    (0..11)
        .map(|n| {
            let mut f = blank(27, 19);
            ring(&mut f, 11.0, 5.0, [255, 220, 60, 120]);
            let a = n as f32 * TAU / 11.0;
            dot(&mut f, (13.5 + 11.0 * a.cos(), 9.5 + 5.0 * a.sin()), 2.5, [255, 250, 180, 240]);
            f
        })
        .collect()
}

fn load_art(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let original = levels.original.then(|| original_frames(&levels.data_dir).map_err(|e| warn!("original status markers: {e}")).ok()).flatten();
    let is_original = original.is_some();
    let [low, blood, hypno, arrow, shield] = original.unwrap_or_else(|| [Vec::new(), generated_bloodlust(), generated_hypnosis(), Vec::new(), generated_shield()]);
    let mut upload = |frames: Vec<Frame>| upload_frames(&frames, &mut images, &mut meshes, &mut mats);
    let (low_health, bloodlust, hypnosis, arrow, shield) = (upload(low), upload(blood), upload(hypno), upload(arrow), upload(shield));
    let shield_tribes = (0..TRIBES)
        .map(|tribe| {
            shield
                .iter()
                .map(|f| {
                    let tinted = StandardMaterial { base_color: crate::sites::tribe_color(tribe), ..mats.get(&f.material).cloned().unwrap_or_default() };
                    mats.add(tinted)
                })
                .collect()
        })
        .collect();
    // Always generated (no original "burning person" sprite identified, unit-art.md): a small flame mesh, like
    // the camp fire's own (`flame::crossed_boards`), cycling through the same generated flame picture's frames.
    let flame_mesh = meshes.add(to_mesh(flame::crossed_boards(BURN_SIZE)));
    let flame_picture = flame::flame_rgba(&flame::generated_tile(), &flame::fallback_alpha_rgba());
    let burning: Vec<FrameAsset> = flame::frame_materials(&flame_picture, &mut images, &mut mats)
        .into_iter()
        .map(|material| FrameAsset { image: Handle::default(), mesh: flame_mesh.clone(), material, size: Vec2::ZERO, origin: Vec2::ZERO })
        .collect();
    commands.insert_resource(MarkerArt { original: is_original, low_health, bloodlust, hypnosis, arrow, shield, shield_tribes, burning });
}

/// Poses every unit's marker slots at the drawn tick, over its body frame.
fn animate_markers(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    selection: Res<Selection>,
    art: Option<Res<MarkerArt>>,
    views: Query<(&UnitView, &Children)>,
    bodies: Query<&BodyHeight>,
    mut slots: Query<(&mut MarkerSlot, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
) {
    let Some(art) = art else { return };
    let n = now.tick.to_wire();
    for (view, children) in &views {
        let Some(u) = map.0.units.get(view.0) else { continue };
        let seen = !hidden_inside(&map.0, u);
        let height = children.iter().find_map(|c| bodies.get(c).ok()).map_or(0.0, |b| b.0);
        for child in children.iter() {
            let Ok((mut slot, mut mesh, mut material, mut vis)) = slots.get_mut(child) else { continue };
            let m = SLOTS[slot.slot];
            let (i, above) = pose(m, n, u.id.raw());
            let frame = (seen && shown(m, u, super::PLAYER, selection.contains(u.id), art.original, now.tick)).then(|| art.frame(m, i, u.owner)).flatten();
            let Some((m_mesh, m_material)) = frame else {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            };
            if mesh.0 != m_mesh {
                mesh.0 = m_mesh;
            }
            if material.0 != m_material {
                material.0 = m_material;
            }
            slot.above = above * height;
            vis.set_if_neq(Visibility::Inherited);
        }
    }
}

pub struct MarkersPlugin;

impl Plugin for MarkersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(Update, animate_markers.after(super::UnitViews).in_set(crate::menu::Gameplay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::status::{Timed, BLINK_STEPS, SHIELD_STEPS};
    use game_core::unit::UnitKind;

    fn brave(owner: u8) -> Unit {
        Unit::new(game_core::id::Id::from_raw(1), owner, UnitKind::Brave, (5000, 5000))
    }

    #[test]
    fn markers_read_the_units_own_state() {
        let mut u = brave(0);
        assert!(SLOTS.iter().all(|&m| !shown(m, &u, 0, false, true, Tick::ZERO)), "nothing on a healthy plain brave");
        u.statuses.shield = Timed::steps(SHIELD_STEPS);
        u.statuses.hypnotized = Timed::steps(game_core::status::HYPNO_STEPS);
        assert!(shown(Marker::Shield(2), &u, 0, false, false, Tick::ZERO) && shown(Marker::Hypnotised, &u, 0, false, false, Tick::ZERO));
        assert!(!shown(Marker::Bloodlust, &u, 0, false, true, Tick::ZERO));
        u.health.damage(u.health.max() * 3 / 4);
        assert!(shown(Marker::LowHealth, &u, 0, false, true, Tick::ZERO), "a quarter left");
        assert!(!shown(Marker::LowHealth, &u, 0, false, false, Tick::ZERO), "only with the original's art");
        u.action = game_core::unit::Action::Dead { left: game_core::time::Countdown::new(game_core::time::Ticks::new(3)) };
        assert!(SLOTS.iter().all(|&m| !shown(m, &u, 0, true, true, Tick::ZERO)), "none once dead");
    }

    #[test]
    fn shield_and_bloodlust_blink_in_their_last_steps() {
        let mut u = brave(0);
        u.statuses.shield = Timed::steps(BLINK_STEPS);
        u.statuses.bloodlust = Timed::steps(SHIELD_STEPS);
        let at = |n: u32| Tick::from_wire(n.wrapping_sub(u.id.raw()));
        assert!(shown(Marker::Shield(0), &u, 0, false, false, at(0)) && !shown(Marker::Shield(0), &u, 0, false, false, at(2)), "2 on, 2 off");
        assert!(shown(Marker::Bloodlust, &u, 0, false, false, at(2)), "steady with time left");
    }

    #[test]
    fn the_arrow_only_over_the_players_selected_people_with_the_originals_art() {
        assert!(shown(Marker::Selected, &brave(0), 0, true, true, Tick::ZERO));
        assert!(!shown(Marker::Selected, &brave(0), 0, false, true, Tick::ZERO), "not selected");
        assert!(!shown(Marker::Selected, &brave(1), 0, true, true, Tick::ZERO), "another tribe's");
        assert!(!shown(Marker::Selected, &brave(0), 0, true, false, Tick::ZERO), "fallback: the health bar instead");
    }

    #[test]
    fn markers_loop_over_their_frames_at_their_height() {
        assert_eq!((pose(Marker::LowHealth, 13, 0), pose(Marker::Bloodlust, 9, 0)), ((3, 1.11), (9, 1.11)));
        assert_eq!(pose(Marker::Hypnotised, 11, 0), (0, 1.25));
        assert_eq!(pose(Marker::Selected, 7, 0), (0, 1.0));
    }

    #[test]
    fn shield_rings_rise_and_fall_one_step_apart_each_person_at_its_phase() {
        let ring0: Vec<(usize, f32)> = (0..16).map(|n| pose(Marker::Shield(0), n, 0)).collect();
        assert_eq!((ring0[0], ring0[4], ring0[8]), ((0, 1.0 / 8.0), (4, (1.0 + 3.5) / 8.0), (0, 1.0)), "feet, mid-height largest, head");
        assert_eq!(ring0[12], ring0[4], "back down");
        assert_eq!(pose(Marker::Shield(1), 5, 0), (ring0[4].0 + 5, ring0[4].1), "ring 1 one step behind, its own sprites");
        assert_eq!(pose(Marker::Shield(2), 0, 0), (SHIELD_PATH[14].0 + 10, pose(Marker::Shield(0), 14, 0).1), "wraps");
        assert_eq!(pose(Marker::Shield(0), 3, 7), pose(Marker::Shield(0), 10, 0), "per person phase");
    }

    #[test]
    fn stand_ins_match_the_originals_counts_and_sizes() {
        let check = |frames: Vec<Frame>, count: usize, size: (usize, usize)| {
            assert_eq!(frames.len(), count);
            assert!(frames.iter().all(|f| (f.width, f.height) == size && f.rgba.len() == size.0 * size.1 * 4 && f.rgba.chunks(4).any(|p| p[3] > 0)));
        };
        check(generated_shield(), 15, (32, 16));
        check(generated_bloodlust(), 10, (30, 25));
        check(generated_hypnosis(), 11, (27, 19));
        let shield = generated_shield();
        let alpha = |f: &Frame| f.rgba.chunks(4).map(|p| p[3] as u32).sum::<u32>();
        assert!(alpha(&shield[2]) > alpha(&shield[7]) && alpha(&shield[7]) > alpha(&shield[12]), "rings bright, medium, faint");
    }
}
