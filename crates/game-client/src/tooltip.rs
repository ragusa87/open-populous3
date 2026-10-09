//! The rendered tooltip of a building (docs/specs/tooltips.md): its name, a line or two of text, then
//! slot rows of icons, one per place or piece (`GameMap::people_slots`, `wood_slots`): the people in
//! it or at work on it, its wood. Long rows wrap (`line_lengths`). It stands above the building and
//! stays while the cursor is on the building or on it (`sticky`), so a person's slot can be clicked
//! to add that unit to the selection. The icons are generated here, a silhouette per unit kind in its tribe's
//! colour, grey when a slot is empty, a log for wood.

use crate::buildings::{BuildingView, ModelHeights};
use crate::camera::GameCamera;
use crate::units::selection::Selection;
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::building::Reward;
use game_core::map::GameMap;
use game_core::occupancy::{Holder, People, Wood};
use game_core::unit::UnitKind;
use std::collections::HashMap;

/// Icon size in pixels, and how much bigger it is drawn.
pub const ICON_W: usize = 12;
pub const ICON_H: usize = 16;
const ICON_SCALE: f32 = 1.5;
/// A row wraps after this many slots, a row of `EIGHTS` after 8.
const LINE: usize = 10;
const EIGHTS: usize = 16;
/// Seconds the tooltip stays once neither its building nor itself has the cursor, to cross the gap.
pub const GRACE_SECS: f32 = 0.3;
/// Pixels between the top of the building and the tooltip.
const ABOVE: f32 = 8.0;

/// What a slot shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// A unit of this kind, in its tribe's colour.
    Unit { kind: UnitKind, tribe: u8 },
    /// An empty place for a unit of this kind (greyed).
    Placeholder(UnitKind),
    Wood,
    /// A piece of wood still missing (greyed).
    NoWood,
}

/// One slot: its icon, and the unit a click on it selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub icon: Icon,
    pub unit: Option<u32>,
}

/// A tooltip: text lines (the name first), then rows of slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TooltipModel {
    pub lines: Vec<String>,
    pub rows: Vec<Vec<Slot>>,
}

/// How many slots each line of a row of `n` holds: lines of 10, but a row of 16 as two of 8.
pub fn line_lengths(n: usize) -> Vec<usize> {
    let width = if n == EIGHTS { 8 } else { LINE };
    (0..n.div_ceil(width)).map(|k| width.min(n - k * width)).collect()
}

/// A people row: the filled places with their unit's icon (a click selects it), the others with
/// `placeholder`.
pub fn people_row(map: &GameMap, people: &People, placeholder: UnitKind) -> Vec<Slot> {
    let filled = people.filled.iter().filter_map(|id| map.units.iter().find(|u| u.id == *id)).map(|u| Slot { icon: Icon::Unit { kind: u.kind, tribe: u.owner }, unit: Some(u.id) });
    let empty = std::iter::repeat_n(Slot { icon: Icon::Placeholder(placeholder), unit: None }, people.capacity as usize - people.filled.len());
    filled.chain(empty).collect()
}

pub fn wood_row(wood: Wood) -> Vec<Slot> {
    let slot = |icon| Slot { icon, unit: None };
    std::iter::repeat_n(slot(Icon::Wood), wood.filled as usize).chain(std::iter::repeat_n(slot(Icon::NoWood), (wood.capacity - wood.filled) as usize)).collect()
}

fn reward_name(reward: Reward) -> String {
    match reward {
        Reward::Spell(kind) => kind.name().to_string(),
        Reward::Building(kind) => kind.name(),
    }
}

/// Building `i`'s tooltip: its name; a pyramid also what it teaches (until granted) and its shaman
/// place; the player's buildings that take wood their people and wood rows; other buildings their
/// name only.
pub fn building_model(map: &GameMap, i: usize) -> TooltipModel {
    let b = &map.buildings[i];
    let mut model = TooltipModel { lines: vec![b.kind.name()], rows: Vec::new() };
    let holder = Holder::Building(i);
    if let Some(vault) = b.vault {
        if let Some(reward) = vault.reward.filter(|_| !vault.is_spent()) {
            model.lines.push(format!("Teaches: {}", reward_name(reward)));
        }
        model.rows.extend(map.people_slots(holder).map(|p| people_row(map, &p, UnitKind::Shaman)));
        return model;
    }
    if b.owner != crate::units::PLAYER || b.kind.wood_cost() == 0 {
        return model;
    }
    model.rows.extend(map.people_slots(holder).map(|p| people_row(map, &p, UnitKind::Brave)));
    model.rows.extend(map.wood_slots(holder).map(wood_row));
    model
}

/// Which building's tooltip shows, from the one `shown`, the one under the cursor (`hovered`, `rested`
/// on long enough), whether the cursor is on the tooltip, and since when (`lost`) neither has it.
/// A shown tooltip stays while the cursor is on its building or on it, and `GRACE_SECS` after.
pub fn sticky(shown: Option<usize>, hovered: Option<usize>, rested: bool, over_tooltip: bool, lost_for: f32) -> Option<usize> {
    if over_tooltip || (shown.is_some() && hovered == shown) {
        return shown;
    }
    match hovered {
        Some(h) if rested => Some(h),
        _ if hovered.is_none() && lost_for < GRACE_SECS => shown,
        _ => None,
    }
}

fn rgba(c: Color, alpha: u8) -> [u8; 4] {
    let s = c.to_srgba();
    [(s.red * 255.0) as u8, (s.green * 255.0) as u8, (s.blue * 255.0) as u8, alpha]
}

/// The icon's pixels, `ICON_W` x `ICON_H` RGBA rows from the top: a person (head, body, legs) shaped
/// by kind (a staff for the shaman, broad shoulders for warriors, a robe for preachers, a flame for
/// firewarriors), or a log.
pub fn icon_pixels(icon: Icon) -> Vec<u8> {
    const GREY: [u8; 4] = [150, 150, 150, 150];
    let mut px = vec![0u8; ICON_W * ICON_H * 4];
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        if x < ICON_W && y < ICON_H {
            px[(y * ICON_W + x) * 4..][..4].copy_from_slice(&c);
        }
    };
    let (kind, body) = match icon {
        Icon::Wood | Icon::NoWood => {
            let (bark, ring) = if icon == Icon::Wood { ([120, 74, 36, 255], [196, 150, 90, 255]) } else { (GREY, GREY) };
            for y in 6..11 {
                for x in 1..11 {
                    put(x, y, if x >= 9 { ring } else { bark });
                }
            }
            return px;
        }
        Icon::Unit { kind, tribe } => (kind, rgba(crate::sites::tribe_color(if kind == UnitKind::Wildman { 4 } else { tribe }), 255)),
        Icon::Placeholder(kind) => (kind, GREY),
    };
    let skin = if matches!(icon, Icon::Placeholder(_)) { GREY } else { [222, 172, 128, 255] };
    for y in 1..5 {
        for x in 4..8 {
            if !((y == 1 || y == 4) && (x == 4 || x == 7)) {
                put(x, y, skin);
            }
        }
    }
    let (shoulders, hem) = match kind {
        UnitKind::Warrior => (5, 3),
        UnitKind::Preacher | UnitKind::Shaman => (3, 4),
        UnitKind::Spy => (2, 2),
        _ => (3, 3),
    };
    for y in 5..12 {
        let half = if y < 8 { shoulders } else { hem };
        for x in 6usize.saturating_sub(half)..(6 + half).min(ICON_W) {
            put(x, y, body);
        }
    }
    let legs = if matches!(kind, UnitKind::Preacher | UnitKind::Shaman) { 12..14 } else { 12..16 };
    for y in legs {
        for x in [4, 5, 7, 8] {
            put(x, y, body);
        }
    }
    if kind == UnitKind::Shaman {
        for y in 0..16 {
            put(10, y, if matches!(icon, Icon::Placeholder(_)) { GREY } else { [150, 110, 60, 255] });
        }
    }
    if kind == UnitKind::Firewarrior && !matches!(icon, Icon::Placeholder(_)) {
        put(6, 0, [255, 140, 30, 255]);
    }
    px
}

/// Icon images, made the first time each is shown.
#[derive(Resource, Default)]
struct IconImages(HashMap<Icon, Handle<Image>>);

impl IconImages {
    fn get(&mut self, icon: Icon, images: &mut Assets<Image>) -> Handle<Image> {
        self.0
            .entry(icon)
            .or_insert_with(|| {
                let mut image = Image::new(Extent3d { width: ICON_W as u32, height: ICON_H as u32, depth_or_array_layers: 1 }, TextureDimension::D2, icon_pixels(icon), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { mag_filter: ImageFilterMode::Nearest, min_filter: ImageFilterMode::Nearest, ..default() });
                images.add(image)
            })
            .clone()
    }
}

/// The building tooltip's box.
#[derive(Component)]
struct BuildingTooltip;

/// A person's slot: a click selects this unit.
#[derive(Component)]
struct SlotUnit(u32);

/// What the tooltip shows, and since when its building and itself lost the cursor.
#[derive(Default)]
struct TooltipState {
    shown: Option<usize>,
    model: Option<TooltipModel>,
    hovered_since: (Option<usize>, f32),
    lost_at: Option<f32>,
}

pub struct TooltipPlugin;

impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconImages>()
            .add_systems(Startup, spawn_tooltip)
            .add_systems(Update, (show_building_tooltip.after(crate::hover::HoverSystems), select_from_slot).in_set(crate::menu::Gameplay));
    }
}

fn spawn_tooltip(mut commands: Commands) {
    commands.spawn((
        BuildingTooltip,
        Interaction::default(),
        BackgroundColor(Color::srgba(0.1, 0.07, 0.03, 0.85)),
        Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::axes(px(6), px(4)), ..default() },
        GlobalZIndex(i32::MAX - 1),
        Visibility::Hidden,
    ));
}

/// The tooltip of the building under the cursor once rested on for `HOVER_SECS`, above it, kept while
/// the cursor is on the building or the tooltip (`sticky`), redrawn when its model changes.
#[allow(clippy::too_many_arguments)]
fn show_building_tooltip(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<CurrentMap>,
    hovered: Res<crate::hover::Hovered>,
    heights: Res<ModelHeights>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    views: Query<(&BuildingView, &GlobalTransform)>,
    mut state: Local<TooltipState>,
    mut icons: ResMut<IconImages>,
    mut images: ResMut<Assets<Image>>,
    mut tooltip: Query<(Entity, &Interaction, &ComputedNode, &mut Node, &mut Visibility), With<BuildingTooltip>>,
) {
    let Ok((entity, interaction, computed, mut node, mut vis)) = tooltip.single_mut() else { return };
    let now = time.elapsed_secs();
    let building = match hovered.0 {
        Some(crate::hover::HoverTarget::Building(i)) if i < map.0.buildings.len() => Some(i),
        _ => None,
    };
    if building != state.hovered_since.0 {
        state.hovered_since = (building, now);
    }
    let over = state.shown.is_some() && *interaction != Interaction::None;
    let holding = over || (building.is_some() && building == state.shown);
    state.lost_at = if holding { None } else { state.lost_at.or(Some(now)) };
    let rested = now - state.hovered_since.1 >= crate::nature::HOVER_SECS;
    let lost_for = state.lost_at.map_or(0.0, |t| now - t);
    state.shown = sticky(state.shown, building, rested, over, lost_for).filter(|&i| i < map.0.buildings.len());
    let anchor = state.shown.and_then(|i| {
        let b = &map.0.buildings[i];
        let (_, gt) = views.iter().find(|(v, _)| v.0 == i)?;
        let (cam, cam_t) = cams.single().ok()?;
        let top = heights.0.get(&(b.x, b.z)).copied().unwrap_or(1.0);
        cam.world_to_viewport(cam_t, gt.translation() + gt.up() * top).ok()
    });
    let (Some(i), Some(at)) = (state.shown, anchor) else {
        vis.set_if_neq(Visibility::Hidden);
        state.model = None;
        return;
    };
    let size = computed.size() * computed.inverse_scale_factor();
    (node.left, node.top) = (px(at.x - size.x / 2.0), px(at.y - size.y - ABOVE));
    vis.set_if_neq(Visibility::Inherited);
    let model = building_model(&map.0, i);
    if state.model.as_ref() == Some(&model) {
        return;
    }
    commands.entity(entity).despawn_children();
    commands.entity(entity).with_children(|t| {
        for (k, line) in model.lines.iter().enumerate() {
            t.spawn((Text::new(line.clone()), TextFont { font_size: FontSize::Px(if k == 0 { 14.0 } else { 12.0 }), ..default() }, TextColor(Color::WHITE), TextShadow::default(), Pickable::IGNORE));
        }
        for row in &model.rows {
            t.spawn((Node { height: px(1), margin: UiRect::vertical(px(1)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)), Pickable::IGNORE));
            let mut start = 0;
            for len in line_lengths(row.len()) {
                t.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(1), ..default() }, Pickable::IGNORE)).with_children(|line| {
                    for slot in &row[start..start + len] {
                        let mut s = line.spawn((ImageNode::new(icons.get(slot.icon, &mut images)), Node { width: px(ICON_W as f32 * ICON_SCALE), height: px(ICON_H as f32 * ICON_SCALE), ..default() }));
                        if let Some(id) = slot.unit {
                            s.insert((SlotUnit(id), Interaction::default()));
                        } else {
                            s.insert(Pickable::IGNORE);
                        }
                    }
                });
                start += len;
            }
        }
    });
    state.model = Some(model);
}

/// A click on a person's slot adds that unit to the selection.
fn select_from_slot(slots: Query<(&Interaction, &SlotUnit), Changed<Interaction>>, mut selection: ResMut<Selection>) {
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            selection.add(slot.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::building::{BuildingKind, Stage};

    fn icons(row: &[Slot]) -> Vec<Icon> {
        row.iter().map(|s| s.icon).collect()
    }

    #[test]
    fn rows_wrap_at_ten_and_sixteen_at_eight() {
        assert_eq!(line_lengths(6), vec![6]);
        assert_eq!(line_lengths(10), vec![10]);
        assert_eq!(line_lengths(12), vec![10, 2]);
        assert_eq!(line_lengths(16), vec![8, 8]);
        assert_eq!(line_lengths(20), vec![10, 10]);
        assert!(line_lengths(0).is_empty());
    }

    #[test]
    fn a_site_shows_its_braves_and_wood_as_slots() {
        let mut map = GameMap::sandbox_buildings();
        let site = map.place_building(crate::units::PLAYER, BuildingKind::Hut { size: 1 }, (88 * 512, 64 * 512), 0).unwrap();
        let brave = map.units.iter().find(|u| u.kind == UnitKind::Brave).unwrap().id;
        map.apply(&game_core::command::Command::OrderUnit { player: 0, unit: brave, order: game_core::unit::Order::Build { site: (88 * 512, 64 * 512) } });
        (map.buildings[site].stock, map.buildings[site].used) = (1, 1);
        let model = building_model(&map, site);
        assert_eq!(model.lines, vec!["Hut 1".to_string()]);
        let people = &model.rows[0];
        assert_eq!(people.len(), 6, "max braves");
        assert_eq!(people[0], Slot { icon: Icon::Unit { kind: UnitKind::Brave, tribe: 0 }, unit: Some(brave) }, "a click selects him");
        assert_eq!(people[1], Slot { icon: Icon::Placeholder(UnitKind::Brave), unit: None });
        assert_eq!(icons(&model.rows[1]), vec![Icon::Wood, Icon::Wood, Icon::NoWood], "2 of 3 provided, nothing to select");
        assert!(model.rows[1].iter().all(|s| s.unit.is_none()));
    }

    #[test]
    fn a_full_hut_and_others_buildings() {
        let map = GameMap::sandbox_buildings();
        let busy = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside > 0).unwrap();
        let model = building_model(&map, busy);
        assert!(model.rows[0].iter().all(|s| matches!(s.icon, Icon::Unit { .. }) && s.unit.is_some()), "3 inside out of 3");
        assert_eq!(icons(&model.rows[1]), vec![Icon::Wood; 3], "the wood in it");
        let red = map.buildings.iter().position(|b| b.owner == 1).unwrap();
        assert!(building_model(&map, red).rows.is_empty(), "not theirs: the name only");
    }

    #[test]
    fn a_pyramid_names_its_reward_until_granted_and_has_a_shaman_place() {
        let mut map = GameMap::sandbox_worship();
        let v = map.buildings.iter().position(|b| b.kind == BuildingKind::Vault).unwrap();
        let model = building_model(&map, v);
        assert_eq!(model.lines[1], "Teaches: Temple");
        assert_eq!(model.rows, vec![vec![Slot { icon: Icon::Placeholder(UnitKind::Shaman), unit: None }]]);
        map.buildings[v].vault.as_mut().unwrap().grant();
        let spent = building_model(&map, v);
        assert_eq!((spent.lines.len(), spent.rows.len()), (1, 0), "granted: its name only");
    }

    #[test]
    fn the_tooltip_stays_while_its_building_or_itself_has_the_cursor() {
        assert_eq!(sticky(None, Some(3), false, false, 0.0), None, "not rested yet");
        assert_eq!(sticky(None, Some(3), true, false, 0.0), Some(3), "rested: shown");
        assert_eq!(sticky(Some(3), Some(3), false, false, 0.0), Some(3), "still on it");
        assert_eq!(sticky(Some(3), None, false, true, 5.0), Some(3), "on the tooltip");
        assert_eq!(sticky(Some(3), None, false, false, GRACE_SECS / 2.0), Some(3), "crossing the gap");
        assert_eq!(sticky(Some(3), None, false, false, GRACE_SECS), None, "focus lost");
        assert_eq!(sticky(Some(3), Some(5), false, false, 0.0), None, "another building, not rested yet");
        assert_eq!(sticky(Some(3), Some(5), true, false, 0.0), Some(5));
    }

    #[test]
    fn icons_are_drawn_filled_or_greyed() {
        let pixels = |icon| icon_pixels(icon).chunks(4).filter(|p| p[3] > 0).count();
        let brave = Icon::Unit { kind: UnitKind::Brave, tribe: 0 };
        assert!(pixels(brave) > 30);
        assert_eq!(pixels(brave), pixels(Icon::Placeholder(UnitKind::Brave)), "same shape");
        assert!(pixels(Icon::Unit { kind: UnitKind::Shaman, tribe: 0 }) > pixels(brave), "with her staff");
        assert_ne!(icon_pixels(brave), icon_pixels(Icon::Unit { kind: UnitKind::Brave, tribe: 1 }), "tribe colour");
        assert_eq!(icon_pixels(Icon::Wood).len(), ICON_W * ICON_H * 4);
    }
}
