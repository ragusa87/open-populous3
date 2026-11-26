//! The reward icon over a pyramid of knowledge (pop3-rev-analysis.md "The reward icon over a vault"): the panel's
//! glyph of what it teaches (`hfx0-0.dat`, `icon_sprite`) as a fixed camera-facing billboard over the vault, and
//! under it, 80 lower, an animated glow (hfx 1417-1432). The original's stands 1072 over the ground, inside our
//! pyramid (1746 high): ours stands `OVER_TOP` above the pyramid's own top instead, 1072 without the model. Both go as the pyramid's top
//! starts closing over the reward given (the player's choice: the original's go on the turn the vault fires, its
//! gauge full). Without the original files, the generated spell glyph where there is one, no glow.
//! Ours: the glow's sprite per moment (one round of its 16 sprites every `GLOW_LOOP`), no ground light.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::SimNow;
use crate::units::art::{alpha_picture_frame, picture_frame, Frame};
use crate::units::{upload_frames, FrameAsset};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::building::{Building, BuildingKind, Reward};
use game_core::map::GameMap;
use game_core::spell_book::SpellKind;
use game_core::time::Ticks;
use game_core::vault::{Vault, VaultPhase};
use pop3_format::blend::AlphaTable;
use pop3_format::catalog::EFFECT_SPRITE_FILE;
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::collections::BTreeMap;
use std::path::Path;

/// The original's icon height over the vault's ground, the icon over the pyramid's top (ours) and the glow under
/// the icon (object units, drawn as the building is: 512 a cell times the Buildings scale).
const ICON_HEIGHT: f32 = 1072.0;
const OVER_TOP: f32 = 160.0;
const GLOW_BELOW: f32 = 80.0;
const GLOW_SPRITES: std::ops::RangeInclusive<usize> = 1417..=1432;
const GLOW_LOOP: Ticks = Ticks::millis(500);
/// The generated glyph's size (cells).
const GENERATED_SIZE: f32 = 0.6;

/// The `hfx0-0.dat` sprite showing `reward` over a vault.
pub fn icon_sprite(reward: Reward) -> Option<usize> {
    match reward {
        Reward::Spell(kind) => Some(match kind.model() {
            m @ 1..=17 => 1056 + m as usize,
            18 => 1056,
            19 => 414,
            20 => 415,
            21 => 1074,
            _ => return None,
        }),
        Reward::Building(kind) => Some(match kind {
            BuildingKind::DrumTower => 1076,
            BuildingKind::WarriorTraining => 1077,
            BuildingKind::FirewarriorTraining => 1078,
            BuildingKind::Temple => 1079,
            BuildingKind::SpyTraining => 1080,
            BuildingKind::BoatHut => 1081,
            BuildingKind::AirshipHut => 1082,
            BuildingKind::GuardPost => 1083,
            _ => 1075,
        }),
        _ => None,
    }
}

/// How high the icon stands over the ground (object units): above `pyramid`'s top, or the original's height without it.
pub fn icon_height(pyramid: Option<&pop3_format::Object>) -> f32 {
    pyramid.and_then(|o| o.points.iter().map(|p| p[1]).max()).map_or(ICON_HEIGHT, |top| top as f32 + OVER_TOP)
}

/// The middle of `pyramid`'s head (object units, x and z in its own frame): the mean of its points in the top
/// quarter of its height, the petals round its open top; the model's origin without it.
pub fn head_centre(pyramid: Option<&pop3_format::Object>) -> (f32, f32) {
    let Some(o) = pyramid else { return (0.0, 0.0) };
    let top = o.points.iter().map(|p| p[1]).max().unwrap_or(0) as f32;
    let head: Vec<_> = o.points.iter().filter(|p| p[1] as f32 >= top * 0.75).collect();
    if head.is_empty() {
        return (0.0, 0.0);
    }
    let n = head.len() as f32;
    (head.iter().map(|p| p[0] as f32).sum::<f32>() / n, head.iter().map(|p| p[2] as f32).sum::<f32>() / n)
}

/// Every sprite `icon_sprite` may name.
fn icon_sprites() -> impl Iterator<Item = usize> {
    (1056..=1083).chain([414, 415])
}

/// The icon is up: the vault has something to teach and its top has not started closing over it.
pub fn shows_icon(vault: &Vault) -> bool {
    vault.reward.is_some() && vault.phase != VaultPhase::Spent && vault.door_and_top().1 == game_core::vault::FULL
}

/// Which glow sprite shows `secs` into the game.
pub fn glow_frame(secs: f32, frames: usize) -> usize {
    let per = GLOW_LOOP.get() as f32 * crate::sim_time::TICK_SECS / frames.max(1) as f32;
    ((secs / per).floor().max(0.0) as usize) % frames.max(1)
}

#[derive(Resource, Default)]
pub struct VaultIconAssets {
    icons: BTreeMap<usize, FrameAsset>,
    glow: Vec<FrameAsset>,
    generated: BTreeMap<SpellKind, Handle<StandardMaterial>>,
    quad: Handle<Mesh>,
}

fn original_frames(data_dir: &Path) -> Result<(Vec<(usize, Frame)>, Vec<Frame>), String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let lut = AlphaTable::load(data_dir, 0).map_err(err)?.rgba(&palette);
    let sprite = |i: usize| bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"));
    let centred = |s: &pop3_format::Sprite| Picture { sprite: s.clone(), origin: (s.width / 2, s.height / 2) };
    let icons = icon_sprites().map(|i| Ok((i, picture_frame(&centred(sprite(i)?), &palette)))).collect::<Result<_, String>>()?;
    let glow = GLOW_SPRITES.map(|i| Ok(alpha_picture_frame(&centred(sprite(i)?), &lut))).collect::<Result<_, String>>()?;
    Ok((icons, glow))
}

fn load_assets(levels: Res<LevelList>, server: Res<AssetServer>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut assets = VaultIconAssets { quad: meshes.add(Rectangle::new(GENERATED_SIZE, GENERATED_SIZE)), ..default() };
    if let Some((icons, glow)) = levels.original.then(|| original_frames(&levels.data_dir).map_err(|e| warn!("original vault icons: {e}")).ok()).flatten() {
        let frames: Vec<Frame> = icons.iter().map(|(_, f)| f.clone()).collect();
        assets.icons = icons.iter().map(|(i, _)| *i).zip(upload_frames(&frames, &mut images, &mut meshes, &mut mats)).collect();
        assets.glow = upload_frames(&glow, &mut images, &mut meshes, &mut mats);
    } else {
        for &kind in SpellKind::ALL.iter() {
            if let Some(path) = crate::hud::spells::generated_glyph_path(kind) {
                let texture = server.load(format!("spell-hud/{path}.png"));
                assets.generated.insert(kind, mats.add(StandardMaterial { base_color_texture: Some(texture), alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() }));
            }
        }
    }
    commands.insert_resource(assets);
}

#[derive(Component)]
pub struct VaultIconView {
    icon: Entity,
    glow: Entity,
}

impl WorldView for VaultIconView {
    type Model = Building;
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, CameraRig>,
        Res<'static, CurveParamsRes>,
        Res<'static, VaultIconAssets>,
        Res<'static, crate::object_scale::ObjectScale>,
        Res<'static, crate::original_models::OriginalObjects>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility, &'static mut Mesh3d, &'static mut MeshMaterial3d<StandardMaterial>)>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Building> {
        map.buildings.iter().filter(|b| b.vault.as_ref().is_some_and(shows_icon))
    }

    fn id(b: &Building) -> u32 {
        b.id.raw()
    }

    fn spawn(b: &Building, view: &mut EntityCommands, (_, _, _, assets, _, _, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let reward = b.vault?.reward?;
        let (icon_mesh, icon_mat) = match icon_sprite(reward).and_then(|i| assets.icons.get(&i)) {
            Some(f) => (f.mesh.clone(), f.material.clone()),
            None => {
                let Reward::Spell(kind) = reward else { return None };
                (assets.quad.clone(), assets.generated.get(&kind)?.clone())
            }
        };
        let glow = assets.glow.first().map(|f| (f.mesh.clone(), f.material.clone())).unwrap_or((assets.quad.clone(), icon_mat.clone()));
        view.insert((Transform::default(), Visibility::Visible));
        let (mut icon, mut glow_e) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        view.with_children(|v| {
            glow_e = v.spawn((Mesh3d(glow.0), MeshMaterial3d(glow.1), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
            icon = v.spawn((Mesh3d(icon_mesh), MeshMaterial3d(icon_mat), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
        });
        Some(VaultIconView { icon, glow: glow_e })
    }

    fn update(&self, b: &Building, now: SimNow, (map, rig, params, assets, objects, models, parts): &mut SystemParamItem<Self::Params>) {
        let cell = WORLD_UNITS_PER_CELL as f32;
        let (x, z) = b.centre();
        let ground = render_pos(&map.0.terrain, &Grounded { at: Vec2::new(x as f32, z as f32) / cell, half: 0.0 }, rig.focus, &params.0);
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let scale = objects.get(crate::object_scale::ObjectGroup::Buildings) / cell;
        let glow = (!assets.glow.is_empty()).then(|| &assets.glow[glow_frame(now.secs_since(game_core::time::Tick::ZERO), assets.glow.len())]);
        let pyramid = models.0.as_ref().and_then(|bank| bank.get(pop3_format::catalog::KNOWLEDGE_PYRAMID));
        let height = icon_height(pyramid);
        let (hx, hz) = head_centre(pyramid);
        let head = Quat::from(crate::buildings::facing_yaw(b.facing)) * Vec3::from(crate::game_frame::GamePos(Vec3::new(hx, 0.0, hz) * scale));
        for (part, lift, frame) in [(self.icon, height, None), (self.glow, height - GLOW_BELOW, glow)] {
            let Ok((mut t, mut vis, mut mesh, mut mat)) = parts.get_mut(part) else { continue };
            match ground {
                Some(g) if part == self.icon || frame.is_some() => {
                    (t.translation, t.rotation, *vis) = (g + head + Vec3::Y * lift * scale, facing, Visibility::Inherited);
                    if let Some(f) = frame {
                        (mesh.0, mat.0) = (f.mesh.clone(), f.material.clone());
                    }
                }
                _ => *vis = Visibility::Hidden,
            }
        }
    }
}

pub struct VaultIconPlugin;

impl Plugin for VaultIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<VaultIconView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_the_rewards_panel_glyph() {
        assert_eq!(icon_sprite(Reward::Spell(SpellKind::Blast)), Some(1058));
        assert_eq!(icon_sprite(Reward::Spell(SpellKind::Convert)), Some(1073));
        assert_eq!(icon_sprite(Reward::Building(BuildingKind::WarriorTraining)), Some(1077), "level 1's vault");
        assert_eq!(icon_sprite(Reward::Building(BuildingKind::Hut { size: 2 })), Some(1075));
        assert_eq!(icon_sprite(Reward::Building(BuildingKind::Prison)), Some(1075), "others as a hut");
        assert_eq!(icon_sprite(Reward::Mana(5)), None);
        assert!(SpellKind::ALL.iter().filter_map(|&k| icon_sprite(Reward::Spell(k))).all(|i| icon_sprites().any(|j| j == i)));
    }

    #[test]
    fn the_icon_goes_as_the_top_starts_closing() {
        let mut v = Vault::new(Some(Reward::Spell(SpellKind::Swarm)), 2);
        assert!(shows_icon(&v));
        v.phase = VaultPhase::Praying { progress: v.full() };
        assert!(shows_icon(&v), "full: she walks in");
        assert_eq!(v.grant(), Some(Reward::Spell(SpellKind::Swarm)));
        assert!(shows_icon(&v), "given, the top still open");
        while v.door_and_top().1 == game_core::vault::FULL {
            v.tick(0, false);
        }
        assert!(!shows_icon(&v), "gone as the top starts closing");
        v.phase = VaultPhase::Spent;
        assert!(!shows_icon(&v));
        assert!(!shows_icon(&Vault::new(None, 2)), "nothing to teach");
    }

    #[test]
    fn the_head_centre_is_the_middle_of_the_top_points() {
        let points = vec![[-100, 0, -100], [100, 0, 100], [40, 1000, 60], [60, 1000, 100], [50, 800, 80]];
        let o = pop3_format::Object { points, ..Default::default() };
        assert_eq!(head_centre(Some(&o)), (50.0, 80.0), "the base points left out");
        assert_eq!(head_centre(None), (0.0, 0.0));
    }

    #[test]
    fn the_glow_loops_through_its_sprites() {
        let frames: Vec<usize> = (0..40).map(|k| glow_frame(k as f32 * 0.02, 16)).collect();
        assert!(frames.windows(2).all(|w| w[1] == w[0] || w[1] == (w[0] + 1) % 16));
        assert!(frames.contains(&0) && frames.contains(&15));
    }
}
