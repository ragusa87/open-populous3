//! The rendered tooltip of a building or a tree (docs/specs/tooltips.md): its name, a line or two of
//! text, then slot rows of icons, one per place or piece (`GameMap::people_slots`, `wood_slots`): the
//! people in it or at work on it, its wood. Long rows wrap (`line_lengths`). It shows after resting the
//! cursor on the thing or at once on a right click, stands above it and stays while the cursor is on
//! the thing or on it (`sticky`), so a person's slot can be clicked to add that unit to the selection. The icons are generated here, a silhouette per unit kind in its tribe's
//! colour, grey when a slot is empty, a log for wood.

use crate::buildings::{BuildingView, ModelHeights};
use crate::camera::GameCamera;
use crate::nature::{size_factor, TreeModel, TreeView};
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
/// Height of the arrow over a selected unit's slot, in pixels on screen.
const ARROW_H: f32 = 6.0;

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
    /// The arrow over a selected unit's slot.
    Selected,
}

/// What a tooltip is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Building(usize),
    Tree(usize),
    Totem(usize),
}

/// One slot: its icon, the unit a click on it selects, and whether that unit is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub icon: Icon,
    pub unit: Option<u32>,
    pub selected: bool,
}

/// A tooltip: text lines (the name first), then rows of slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TooltipModel {
    pub lines: Vec<String>,
    pub rows: Vec<Vec<Slot>>,
    pub bars: Vec<Bar>,
}

/// What a bar measures; it sets its colour and where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    /// Prayer progress of the player's tribe (pyramid, totem).
    Prayer,
    /// A house growing to its next size (once houses grow).
    #[allow(dead_code)]
    Growth,
    /// A house making its next brave (once houses breed).
    #[allow(dead_code)]
    Birth,
    /// The unit in a training hut.
    Training,
}

impl BarKind {
    /// Horizontal across the top of the tooltip (training), else vertical on its left.
    pub fn horizontal(self) -> bool {
        self == BarKind::Training
    }

    pub fn color(self) -> Color {
        match self {
            BarKind::Prayer => Color::srgb(0.95, 0.8, 0.3),
            BarKind::Growth => Color::srgb(0.85, 0.25, 0.2),
            BarKind::Birth => Color::srgb(0.3, 0.8, 0.3),
            BarKind::Training => Color::srgb(0.35, 0.6, 1.0),
        }
    }
}

/// A progress bar: `fill` in thousandths, blinking while `blocked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub kind: BarKind,
    pub fill: u16,
    pub blocked: bool,
}

/// How many slots each line of a row of `n` holds: lines of 10, but a row of 16 as two of 8.
pub fn line_lengths(n: usize) -> Vec<usize> {
    let width = if n == EIGHTS { 8 } else { LINE };
    (0..n.div_ceil(width)).map(|k| width.min(n - k * width)).collect()
}

/// A people row: the filled places with their unit's icon (a click selects it, marked when among
/// `selected`), the others with `placeholder`.
pub fn people_row(map: &GameMap, people: &People, placeholder: UnitKind, selected: &[u32]) -> Vec<Slot> {
    let filled = people.filled.iter().filter_map(|id| map.units.iter().find(|u| u.id == *id)).map(|u| Slot { icon: Icon::Unit { kind: u.kind, tribe: u.owner }, unit: Some(u.id), selected: selected.contains(&u.id) });
    let empty = std::iter::repeat_n(Slot { icon: Icon::Placeholder(placeholder), unit: None, selected: false }, people.capacity as usize - people.filled.len());
    filled.chain(empty).collect()
}

pub fn wood_row(wood: Wood) -> Vec<Slot> {
    let slot = |icon| Slot { icon, unit: None, selected: false };
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
/// name only. People among `selected` are marked.
pub fn building_model(map: &GameMap, i: usize, selected: &[u32]) -> TooltipModel {
    let b = &map.buildings[i];
    let mut model = TooltipModel { lines: vec![b.kind.name()], ..default() };
    let holder = Holder::Building(i);
    if let Some(vault) = b.vault {
        if let Some(reward) = vault.reward.filter(|_| !vault.is_spent()) {
            model.lines.push(format!("Teaches: {}", reward_name(reward)));
        }
        model.rows.extend(map.people_slots(holder).map(|p| people_row(map, &p, UnitKind::Shaman, selected)));
        if let game_core::vault::VaultPhase::Praying { progress } = vault.phase {
            let fill = (progress.min(vault.pray_time) as u32 * game_core::vault::FULL as u32 / vault.pray_time as u32) as u16;
            model.bars.push(Bar { kind: BarKind::Prayer, fill, blocked: false });
        }
        return model;
    }
    if b.owner != crate::units::PLAYER || b.kind.wood_cost() == 0 {
        return model;
    }
    model.rows.extend(map.people_slots(holder).map(|p| people_row(map, &p, UnitKind::Brave, selected)));
    model.rows.extend(map.wood_slots(holder).map(wood_row));
    model
}

/// A tree's tooltip: its name and its current wood (none for a tree without wood).
pub fn tree_model(map: &GameMap, i: usize) -> Option<TooltipModel> {
    map.wood_slots(Holder::Tree(i)).map(|wood| TooltipModel { lines: vec!["Tree".to_string()], rows: vec![wood_row(wood)], ..default() })
}

/// A totem's tooltip: its name and its places for prayers (`Totem::prayers`, brave shapes, or the
/// shaman's for those only she prays at), filled by those counted; never its reward.
pub fn totem_model(map: &GameMap, i: usize, selected: &[u32]) -> Option<TooltipModel> {
    let totem = map.totems.get(i)?;
    let placeholder = if totem.shaman_only { UnitKind::Shaman } else { UnitKind::Brave };
    let rows = map.people_slots(Holder::Totem(i)).map(|p| people_row(map, &p, placeholder, selected)).into_iter().collect();
    let bars = vec![Bar { kind: BarKind::Prayer, fill: 0, blocked: false }];
    Some(TooltipModel { lines: vec![totem.kind.name().to_string()], rows, bars })
}

/// What `target` shows; None when it has nothing to show.
fn model_of(map: &GameMap, target: Target, selected: &[u32]) -> Option<TooltipModel> {
    match target {
        Target::Building(i) => (i < map.buildings.len()).then(|| building_model(map, i, selected)),
        Target::Tree(i) => tree_model(map, i),
        Target::Totem(i) => totem_model(map, i, selected),
    }
}

/// Which tooltip shows, from the one `shown`, the thing under the cursor (`hovered`, `rested` on long
/// enough or right-clicked), whether the cursor is on the tooltip, and for how long (`lost_for`)
/// neither has it. A shown tooltip stays while the cursor is on its thing or on it, and `GRACE_SECS`
/// after.
pub fn sticky<T: Copy + PartialEq>(shown: Option<T>, hovered: Option<T>, rested: bool, over_tooltip: bool, lost_for: f32) -> Option<T> {
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
        Icon::Selected => {
            for y in 0..4 {
                for x in 3 + y..9 - y {
                    put(x, y + 12, [255, 230, 120, 255]);
                }
            }
            return px;
        }
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

/// The tooltip's box.
#[derive(Component)]
struct TooltipBox;

/// A person's slot: a click selects this unit.
#[derive(Component)]
struct SlotUnit(u32);

/// What the tooltip shows, since when the cursor is on the thing under it (and whether it was
/// right-clicked), and since when the shown thing and the tooltip lost the cursor.
#[derive(Default)]
struct TooltipState {
    shown: Option<Target>,
    model: Option<TooltipModel>,
    hovered_since: (Option<Target>, f32, bool),
    lost_at: Option<f32>,
}

pub struct TooltipPlugin;

impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconImages>()
            .add_systems(Startup, spawn_tooltip)
            .add_systems(Update, (show_tooltip.after(crate::hover::HoverSystems), select_from_slot, blink_bars).in_set(crate::menu::Gameplay));
    }
}

fn spawn_tooltip(mut commands: Commands) {
    commands.spawn((
        TooltipBox,
        Interaction::default(),
        BackgroundColor(Color::srgba(0.1, 0.07, 0.03, 0.85)),
        Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::axes(px(6), px(4)), ..default() },
        GlobalZIndex(i32::MAX - 1),
        Visibility::Hidden,
    ));
}

/// The tooltip of the building or tree under the cursor once rested on for `HOVER_SECS` or right-clicked,
/// above it, kept while the cursor is on it or on the tooltip (`sticky`), redrawn when its model changes.
#[allow(clippy::too_many_arguments)]
fn show_tooltip(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    map: Res<CurrentMap>,
    hovered: Res<crate::hover::Hovered>,
    heights: Res<ModelHeights>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    views: Query<(&BuildingView, &GlobalTransform)>,
    trees: Query<(&TreeView, &GlobalTransform, &Children)>,
    tree_models: Query<&TreeModel>,
    totems: Query<(&crate::totems::TotemView, &GlobalTransform)>,
    selection: Res<Selection>,
    mut state: Local<TooltipState>,
    mut icons: ResMut<IconImages>,
    mut images: ResMut<Assets<Image>>,
    mut tooltip: Query<(Entity, &Interaction, &ComputedNode, &mut Node, &mut Visibility), With<TooltipBox>>,
) {
    let Ok((entity, interaction, computed, mut node, mut vis)) = tooltip.single_mut() else { return };
    let now = time.elapsed_secs();
    let target = match hovered.0 {
        Some(crate::hover::HoverTarget::Building(i)) if i < map.0.buildings.len() => Some(Target::Building(i)),
        Some(crate::hover::HoverTarget::Tree(i)) if i < map.0.trees.len() => Some(Target::Tree(i)),
        Some(crate::hover::HoverTarget::Totem(i)) if i < map.0.totems.len() => Some(Target::Totem(i)),
        _ => None,
    };
    if target != state.hovered_since.0 {
        state.hovered_since = (target, now, false);
    }
    state.hovered_since.2 |= target.is_some() && mouse.just_pressed(MouseButton::Right);
    let over = state.shown.is_some() && *interaction != Interaction::None;
    let holding = over || (target.is_some() && target == state.shown);
    state.lost_at = if holding { None } else { state.lost_at.or(Some(now)) };
    let rested = state.hovered_since.2 || now - state.hovered_since.1 >= crate::nature::HOVER_SECS;
    let lost_for = state.lost_at.map_or(0.0, |t| now - t);
    state.shown = sticky(state.shown, target, rested, over, lost_for);
    let model = state.shown.and_then(|t| model_of(&map.0, t, &selection.units));
    let anchor = state.shown.and_then(|t| {
        let (cam, cam_t) = cams.single().ok()?;
        let top = match t {
            Target::Building(i) => {
                let b = &map.0.buildings[i];
                let (_, gt) = views.iter().find(|(v, _)| v.0 == i)?;
                gt.translation() + gt.up() * heights.0.get(&(b.x, b.z)).copied().unwrap_or(1.0)
            }
            Target::Tree(i) => {
                let (_, gt, children) = trees.iter().find(|(v, _, _)| v.0 == i)?;
                let full = children.iter().find_map(|c| tree_models.get(c).ok())?.full_height;
                gt.translation() + Vec3::Y * size_factor(map.0.trees[i].size) * full
            }
            Target::Totem(i) => {
                let (view, gt) = totems.iter().find(|(v, _)| v.index == i)?;
                gt.translation() + gt.up() * view.top
            }
        };
        cam.world_to_viewport(cam_t, top).ok()
    });
    let (Some(model), Some(at)) = (model, anchor) else {
        vis.set_if_neq(Visibility::Hidden);
        state.model = None;
        return;
    };
    let size = computed.size() * computed.inverse_scale_factor();
    (node.left, node.top) = (px(at.x - size.x / 2.0), px(at.y - size.y - ABOVE));
    vis.set_if_neq(Visibility::Inherited);
    if state.model.as_ref() == Some(&model) {
        return;
    }
    commands.entity(entity).despawn_children();
    commands.entity(entity).with_children(|t| {
        for bar in model.bars.iter().filter(|b| b.kind.horizontal()) {
            spawn_bar(t, *bar);
        }
        t.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(5), ..default() }, Pickable::IGNORE)).with_children(|r| {
            for bar in model.bars.iter().filter(|b| !b.kind.horizontal()) {
                spawn_bar(r, *bar);
            }
            r.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }, Pickable::IGNORE)).with_children(|t| spawn_content(t, &model, &mut icons, &mut images));
        });
    });
    state.model = Some(model);
}

/// Width of a vertical bar, height of a horizontal one, in pixels.
const BAR_THICKNESS: f32 = 6.0;
/// A bar's fill blinks this many times a second while blocked.
const BLINK_HZ: f32 = 2.0;

/// A bar's filled part, blinking while its bar is blocked.
#[derive(Component)]
struct BarFill {
    color: Color,
    blocked: bool,
}

/// A bar: a dark track and its fill, vertical (filling upwards, as tall as the tooltip's content) or
/// horizontal (filling to the right, as wide as the tooltip).
fn spawn_bar(parent: &mut ChildSpawnerCommands, bar: Bar) {
    let share = percent(bar.fill as f32 * 100.0 / game_core::vault::FULL as f32);
    let (track, fill) = if bar.kind.horizontal() {
        (Node { height: px(BAR_THICKNESS), ..default() }, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: share, ..default() })
    } else {
        (Node { width: px(BAR_THICKNESS), ..default() }, Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: share, ..default() })
    };
    parent.spawn((track, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), Pickable::IGNORE)).with_child((fill, BackgroundColor(bar.kind.color()), BarFill { color: bar.kind.color(), blocked: bar.blocked }, Pickable::IGNORE));
}

/// Blocked bars blink: their fill fades out and back in.
fn blink_bars(time: Res<Time>, mut fills: Query<(&BarFill, &mut BackgroundColor)>) {
    let on = (time.elapsed_secs() * BLINK_HZ).fract() < 0.5;
    for (fill, mut color) in &mut fills {
        let shown = if fill.blocked && !on { fill.color.with_alpha(0.2) } else { fill.color };
        color.set_if_neq(BackgroundColor(shown));
    }
}

/// The tooltip's text lines and slot rows.
fn spawn_content(t: &mut ChildSpawnerCommands, model: &TooltipModel, icons: &mut IconImages, images: &mut Assets<Image>) {
    for (k, line) in model.lines.iter().enumerate() {
        t.spawn((Text::new(line.clone()), TextFont { font_size: FontSize::Px(if k == 0 { 14.0 } else { 12.0 }), ..default() }, TextColor(Color::WHITE), TextShadow::default(), Pickable::IGNORE));
    }
    for row in &model.rows {
        t.spawn((Node { height: px(1), margin: UiRect::vertical(px(1)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)), Pickable::IGNORE));
        let people = row.iter().any(|s| matches!(s.icon, Icon::Unit { .. } | Icon::Placeholder(_)));
        let mut start = 0;
        for len in line_lengths(row.len()) {
            t.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(1), ..default() }, Pickable::IGNORE)).with_children(|line| {
                for slot in &row[start..start + len] {
                    line.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, Pickable::IGNORE)).with_children(|cell| {
                        if people {
                            let arrow = (Node { width: px(ICON_W as f32 * ICON_SCALE), height: px(ARROW_H), ..default() }, Pickable::IGNORE);
                            if slot.selected {
                                cell.spawn((ImageNode::new(icons.get(Icon::Selected, images)).with_rect(Rect::new(0.0, 12.0, ICON_W as f32, 16.0)), arrow));
                            } else {
                                cell.spawn(arrow);
                            }
                        }
                        let mut icon = cell.spawn((ImageNode::new(icons.get(slot.icon, images)), Node { width: px(ICON_W as f32 * ICON_SCALE), height: px(ICON_H as f32 * ICON_SCALE), ..default() }));
                        if let Some(id) = slot.unit {
                            icon.insert((SlotUnit(id), Interaction::default()));
                        } else {
                            icon.insert(Pickable::IGNORE);
                        }
                    });
                }
            });
            start += len;
        }
    }
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
        let model = building_model(&map, site, &[]);
        assert_eq!(model.lines, vec!["Hut 1".to_string()]);
        let people = &model.rows[0];
        assert_eq!(people.len(), 6, "max braves");
        assert_eq!(people[0], Slot { icon: Icon::Unit { kind: UnitKind::Brave, tribe: 0 }, unit: Some(brave), selected: false }, "a click selects him");
        assert!(building_model(&map, site, &[brave]).rows[0][0].selected, "marked once selected");
        assert_eq!(people[1], Slot { icon: Icon::Placeholder(UnitKind::Brave), unit: None, selected: false });
        assert_eq!(icons(&model.rows[1]), vec![Icon::Wood, Icon::Wood, Icon::NoWood], "2 of 3 provided, nothing to select");
        assert!(model.rows[1].iter().all(|s| s.unit.is_none()));
    }

    #[test]
    fn a_full_hut_and_others_buildings() {
        let map = GameMap::sandbox_buildings();
        let busy = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside > 0).unwrap();
        let model = building_model(&map, busy, &[]);
        assert!(model.rows[0].iter().all(|s| matches!(s.icon, Icon::Unit { .. }) && s.unit.is_some()), "3 inside out of 3");
        assert_eq!(icons(&model.rows[1]), vec![Icon::Wood; 3], "the wood in it");
        let red = map.buildings.iter().position(|b| b.owner == 1).unwrap();
        assert!(building_model(&map, red, &[]).rows.is_empty(), "not theirs: the name only");
    }

    #[test]
    fn a_pyramid_names_its_reward_until_granted_and_has_a_shaman_place() {
        let mut map = GameMap::sandbox_worship();
        let v = map.buildings.iter().position(|b| b.kind == BuildingKind::Vault).unwrap();
        let model = building_model(&map, v, &[]);
        assert_eq!(model.lines[1], "Teaches: Temple");
        assert_eq!(model.rows, vec![vec![Slot { icon: Icon::Placeholder(UnitKind::Shaman), unit: None, selected: false }]]);
        assert_eq!(model.bars, vec![Bar { kind: BarKind::Prayer, fill: 0, blocked: false }], "prayer bar, empty");
        map.buildings[v].vault.as_mut().unwrap().phase = game_core::vault::VaultPhase::Praying { progress: 60 };
        assert_eq!(building_model(&map, v, &[]).bars[0].fill, 600, "60 of 100");
        map.buildings[v].vault.as_mut().unwrap().grant();
        let spent = building_model(&map, v, &[]);
        assert_eq!((spent.lines.len(), spent.rows.len(), spent.bars.len()), (1, 0, 0), "granted: its name only");
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
    fn a_tree_shows_its_current_wood_only() {
        let mut map = GameMap::sandbox_buildings();
        map.trees[0].size = 3;
        let model = tree_model(&map, 0).unwrap();
        assert_eq!(model.lines, vec!["Tree".to_string()]);
        assert_eq!(icons(&model.rows[0]), vec![Icon::Wood; 3], "no placeholders: its capacity is not known");
        map.trees[0].size = 0;
        assert_eq!(tree_model(&map, 0), None, "no wood: no tooltip");
    }

    #[test]
    fn a_totem_shows_its_places_for_prayers_never_its_reward() {
        let map = GameMap::sandbox_worship();
        let eight = map.totems.iter().position(|t| t.prayers == 8).unwrap();
        let model = totem_model(&map, eight, &[]).unwrap();
        assert_eq!(model.lines.len(), 1, "its name only");
        assert_eq!(icons(&model.rows[0]), vec![Icon::Placeholder(UnitKind::Brave); 8]);
        let shaman = map.totems.iter().position(|t| t.shaman_only).unwrap();
        assert_eq!(icons(&totem_model(&map, shaman, &[]).unwrap().rows[0]), vec![Icon::Placeholder(UnitKind::Shaman)]);
    }

    #[test]
    fn bars_stand_vertical_on_the_left_but_training_across_the_top() {
        assert!(BarKind::Training.horizontal());
        assert!(![BarKind::Prayer, BarKind::Growth, BarKind::Birth].iter().any(|k| k.horizontal()));
        assert_ne!(BarKind::Growth.color(), BarKind::Birth.color(), "a house's two bars apart");
        let map = GameMap::sandbox_worship();
        assert_eq!(totem_model(&map, 0, &[]).unwrap().bars[0].kind, BarKind::Prayer);
        assert!(tree_model(&map, 0).is_none_or(|m| m.bars.is_empty()), "a tree has no bar");
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
        assert!(pixels(Icon::Selected) > 0 && icon_pixels(Icon::Selected)[..12 * ICON_W * 4].iter().all(|&b| b == 0), "the arrow in the bottom rows");
    }
}
