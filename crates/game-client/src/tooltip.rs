//! The rendered tooltip of a building or a tree (docs/specs/tooltips.md): its name, a line or two of
//! text, then slot rows of icons, one per place or piece (`GameMap::people_slots`, `wood_slots`): the
//! people in it or at work on it, its wood. Long rows wrap (`line_lengths`). It shows after resting the
//! cursor on the thing or at once on a right click, stands above it and stays while the cursor is on
//! the thing or on it (`sticky`), so a person's slot can be clicked to add that unit to the selection. A person is the
//! original teal figure of its kind when allowed (`hfx0-0.dat`, `IconImages`, shared with the panel), greyed when the slot is empty; else, and for wood, the
//! icons are generated here, a silhouette per unit kind in its tribe's colour.

use crate::buildings::{BuildingView, ModelHeights};
use crate::camera::GameCamera;
use crate::nature::{size_factor, TreeModel, TreeView};
use crate::units::selection::Selection;
use crate::world::{CurrentMap, LevelList};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::building::Reward;
use game_core::map::GameMap;
use game_core::occupancy::{Holder, People, Wood};
use game_core::unit::UnitKind;
use pop3_format::catalog::{DISMANTLE_BUTTON, EFFECT_SPRITE_FILE, REBUILD_BUTTON, UNIT_FIGURES};
use pop3_format::{Sprite, SpriteBank};
use std::collections::HashMap;

/// Generated icon size in pixels, drawn 1.5 times bigger in a slot.
pub const ICON_W: usize = 12;
pub const ICON_H: usize = 16;
/// A slot's size in pixels on screen: an original figure is drawn 1:1 in it.
const SLOT_W: usize = 18;
const SLOT_H: usize = 24;
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
    /// The button at the top right.
    pub toggle: Option<Toggle>,
}

/// A tooltip's button (tooltips.md "Dismantle toggle"): dismantle the building at `site` (stored
/// corner), or build it again when `on`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggle {
    pub site: (u16, u16),
    pub on: bool,
}

impl Toggle {
    /// What a click sends.
    pub fn command(self) -> game_core::command::Command {
        game_core::command::Command::Dismantle { player: crate::units::PLAYER, site: self.site, on: !self.on }
    }

    /// Its sprites (normal, hovered, pressed) and its text without the original files.
    pub fn look(self) -> ([usize; 3], &'static str) {
        if self.on { (REBUILD_BUTTON, "Rebuild") } else { (DISMANTLE_BUTTON, "Dismantle") }
    }
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
        Reward::OneShot(kind) => format!("{} (one cast)", kind.name()),
        Reward::Mana(n) => format!("{n} mana"),
        Reward::Unhandled { .. } => "Something unknown".to_string(),
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
        if let game_core::vault::VaultPhase::Praying { .. } = vault.phase {
            model.bars.push(Bar { kind: BarKind::Prayer, fill: vault.progress_permille(), blocked: false });
        }
        return model;
    }
    if b.owner != crate::units::PLAYER || b.kind.wood_cost() == 0 {
        return model;
    }
    model.rows.extend(map.people_slots(holder).map(|p| people_row(map, &p, UnitKind::Brave, selected)));
    model.rows.extend(map.wood_slots(holder).map(wood_row));
    model.toggle = b.flat.then_some(Toggle { site: (b.x, b.z), on: b.dismantling });
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
    let fill = (totem.gauges[crate::units::PLAYER as usize] as u64 * game_core::vault::FULL as u64 / totem.full().max(1) as u64) as u16;
    let bars = if totem.is_exhausted() { Vec::new() } else { vec![Bar { kind: BarKind::Prayer, fill, blocked: false }] };
    Some(TooltipModel { lines: vec![totem.kind.name().to_string()], rows, bars, toggle: None })
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

/// The original teal figure of `kind` in `UNIT_FIGURES`; None for wildmen.
pub fn figure_sprite(kind: UnitKind) -> Option<usize> {
    let k = match kind {
        UnitKind::Brave => 0,
        UnitKind::Warrior => 1,
        UnitKind::Preacher => 2,
        UnitKind::Spy => 3,
        UnitKind::Firewarrior => 4,
        UnitKind::Shaman => 5,
        UnitKind::Wildman => return None,
    };
    Some(UNIT_FIGURES.start() + k)
}

/// A sprite's pixels, `w` x `h` RGBA rows, standing at the bottom centre; greyed (its lightness, half
/// seen through) for an empty place.
pub fn sprite_pixels(sprite: &Sprite, palette: &[[u8; 3]], greyed: bool, (w, h): (usize, usize)) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 4];
    let (dx, dy) = (w.saturating_sub(sprite.width) / 2, h.saturating_sub(sprite.height));
    for (i, p) in sprite.pixels.iter().enumerate() {
        let (x, y) = (i % sprite.width + dx, i / sprite.width + dy);
        let Some(&[r, g, b]) = p.and_then(|p| palette.get(p as usize)) else { continue };
        if x >= w || y >= h {
            continue;
        }
        let grey = ((r as u16 * 3 + g as u16 * 6 + b as u16) / 10) as u8;
        px[(y * w + x) * 4..][..4].copy_from_slice(&if greyed { [grey, grey, grey, 150] } else { [r, g, b, 255] });
    }
    px
}

fn pixel_image(w: usize, h: usize, px: Vec<u8>) -> Image {
    let mut image = Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { mag_filter: ImageFilterMode::Nearest, min_filter: ImageFilterMode::Nearest, ..default() });
    image
}

/// Icon images, made the first time each is shown, and the original effect sprites (`hfx0-0.dat`,
/// teal figures, panel icons) with their palette when allowed: shared by the tooltip and the panel.
#[derive(Resource, Default)]
pub struct IconImages {
    made: HashMap<Icon, Handle<Image>>,
    sprites: HashMap<usize, Handle<Image>>,
    original: Option<(SpriteBank, Vec<[u8; 3]>)>,
}

fn load_sprites(levels: Res<LevelList>, mut icons: ResMut<IconImages>) {
    if !levels.original {
        return;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((SpriteBank::load(&levels.data_dir, EFFECT_SPRITE_FILE)?, pop3_format::Theme::load(&levels.data_dir, 0)?.palette)) };
    icons.original = load().map_err(|e| warn!("original panel sprites: {e}")).ok();
    icons.made.clear();
    icons.sprites.clear();
}

impl IconImages {
    /// Whether the original sprites are there.
    pub fn original(&self) -> bool {
        self.original.is_some()
    }

    /// The icon's pixels and size: the original figure for a person when loaded, else the generated icon.
    fn pixels(&self, icon: Icon) -> (usize, usize, Vec<u8>) {
        let person = match icon {
            Icon::Unit { kind, .. } => Some((kind, false)),
            Icon::Placeholder(kind) => Some((kind, true)),
            _ => None,
        };
        let figure = person.and_then(|(kind, greyed)| {
            let (bank, palette) = self.original.as_ref()?;
            Some(sprite_pixels(bank.sprites.get(figure_sprite(kind)?)?, palette, greyed, (SLOT_W, SLOT_H)))
        });
        figure.map_or_else(|| (ICON_W, ICON_H, icon_pixels(icon)), |px| (SLOT_W, SLOT_H, px))
    }

    pub fn get(&mut self, icon: Icon, images: &mut Assets<Image>) -> Handle<Image> {
        if let Some(h) = self.made.get(&icon) {
            return h.clone();
        }
        let (w, h, px) = self.pixels(icon);
        let handle = images.add(pixel_image(w, h, px));
        self.made.insert(icon, handle.clone());
        handle
    }

    /// Original effect sprite `index` at its own size, None without the original files.
    pub fn sprite(&mut self, index: usize, images: &mut Assets<Image>) -> Option<(Handle<Image>, Vec2)> {
        let (bank, palette) = self.original.as_ref()?;
        let sprite = bank.sprites.get(index)?;
        let size = Vec2::new(sprite.width as f32, sprite.height as f32);
        let handle = match self.sprites.get(&index) {
            Some(h) => h.clone(),
            None => {
                let h = images.add(pixel_image(sprite.width, sprite.height, sprite_pixels(sprite, palette, false, (sprite.width, sprite.height))));
                self.sprites.insert(index, h.clone());
                h
            }
        };
        Some((handle, size))
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
            .add_systems(Startup, (spawn_tooltip, load_sprites))
            .add_systems(Update, (show_tooltip.after(crate::hover::HoverSystems), select_from_slot, press_toggle, blink_bars).in_set(crate::menu::Gameplay));
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
        let text = (Text::new(line.clone()), TextFont { font_size: FontSize::Px(if k == 0 { 14.0 } else { 12.0 }), ..default() }, TextColor(Color::WHITE), TextShadow::default(), Pickable::IGNORE);
        match model.toggle.filter(|_| k == 0) {
            Some(toggle) => {
                t.spawn((Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Start, column_gap: px(8), ..default() }, Pickable::IGNORE)).with_children(|top| {
                    top.spawn(text);
                    spawn_toggle(top, toggle, icons, images);
                });
            }
            None => {
                t.spawn(text);
            }
        }
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
                            let arrow = (Node { width: px(SLOT_W as f32), height: px(ARROW_H), ..default() }, Pickable::IGNORE);
                            if slot.selected {
                                cell.spawn((ImageNode::new(icons.get(Icon::Selected, images)).with_rect(Rect::new(0.0, 12.0, ICON_W as f32, 16.0)), arrow));
                            } else {
                                cell.spawn(arrow);
                            }
                        }
                        let mut icon = cell.spawn((ImageNode::new(icons.get(slot.icon, images)), Node { width: px(SLOT_W as f32), height: px(SLOT_H as f32), ..default() }));
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

/// The tooltip's button, its sprites by state (normal, hovered, pressed) when the original files are there.
#[derive(Component)]
struct ToggleButton {
    toggle: Toggle,
    images: Option<[Handle<Image>; 3]>,
}

fn spawn_toggle(parent: &mut ChildSpawnerCommands, toggle: Toggle, icons: &mut IconImages, images: &mut Assets<Image>) {
    let (sprites, label) = toggle.look();
    let made: Option<Vec<(Handle<Image>, Vec2)>> = sprites.iter().map(|&i| icons.sprite(i, images)).collect();
    match made {
        Some(made) => {
            let size = made[0].1;
            let images = [made[0].0.clone(), made[1].0.clone(), made[2].0.clone()];
            parent.spawn((ImageNode::new(images[0].clone()), Node { width: px(size.x), height: px(size.y), ..default() }, Interaction::default(), ToggleButton { toggle, images: Some(images) }));
        }
        None => {
            parent
                .spawn((Node { padding: UiRect::axes(px(4), px(1)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb(0.62, 0.42, 0.14)), BorderColor::all(Color::srgb(1.0, 0.84, 0.48)), Interaction::default(), ToggleButton { toggle, images: None }))
                .with_child((Text::new(label), TextFont { font_size: FontSize::Px(11.0), ..default() }, TextColor(Color::WHITE), Pickable::IGNORE));
        }
    }
}

/// The button shows its state; pressed, it sends its command.
fn press_toggle(mut buttons: Query<(&Interaction, &ToggleButton, Option<&mut ImageNode>, Option<&mut BackgroundColor>), Changed<Interaction>>, mut map: ResMut<CurrentMap>) {
    for (interaction, button, image, bg) in &mut buttons {
        let state = match interaction {
            Interaction::None => 0,
            Interaction::Hovered => 1,
            Interaction::Pressed => 2,
        };
        if let (Some(images), Some(mut image)) = (&button.images, image) {
            image.image = images[state].clone();
        }
        if let Some(mut bg) = bg {
            bg.0 = [Color::srgb(0.62, 0.42, 0.14), Color::srgb(0.75, 0.52, 0.18), Color::srgb(0.40, 0.25, 0.08)][state];
        }
        if *interaction == Interaction::Pressed {
            map.bypass_change_detection().0.apply(&button.toggle.command());
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
    fn the_players_flat_buildings_have_the_dismantle_toggle() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built).unwrap();
        let site = (map.buildings[hut].x, map.buildings[hut].z);
        let toggle = building_model(&map, hut, &[]).toggle.expect("built: dismantle");
        assert_eq!(toggle, Toggle { site, on: false });
        assert_eq!(toggle.look(), (DISMANTLE_BUTTON, "Dismantle"));
        map.apply(&toggle.command());
        let toggle = building_model(&map, hut, &[]).toggle.unwrap();
        assert_eq!((toggle.on, toggle.look()), (true, (REBUILD_BUTTON, "Rebuild")));
        let plan = map.buildings.iter().position(|b| b.owner == 0 && b.stage() == Stage::Blueprint).unwrap();
        assert_eq!(building_model(&map, plan, &[]).toggle, None, "a plan is cancelled instead");
        let red = map.buildings.iter().position(|b| b.owner == 1).unwrap();
        assert_eq!(building_model(&map, red, &[]).toggle, None);
    }

    #[test]
    fn a_pyramid_names_its_reward_until_granted_and_has_a_shaman_place() {
        let mut map = GameMap::sandbox_worship();
        let v = map.buildings.iter().position(|b| b.kind == BuildingKind::Vault).unwrap();
        let model = building_model(&map, v, &[]);
        assert_eq!(model.lines[1], "Teaches: Temple");
        assert_eq!(model.rows, vec![vec![Slot { icon: Icon::Placeholder(UnitKind::Shaman), unit: None, selected: false }]]);
        assert_eq!(model.bars, vec![Bar { kind: BarKind::Prayer, fill: 0, blocked: false }], "prayer bar, empty");
        map.buildings[v].vault.as_mut().unwrap().phase = game_core::vault::VaultPhase::Praying { progress: 60 * game_core::gauge::STEP };
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

    #[test]
    fn a_figure_stands_at_the_bottom_centre_greyed_when_empty() {
        let sprite = Sprite { width: 2, height: 1, pixels: vec![Some(1), None] };
        let palette = [[0, 0, 0], [0, 200, 180]];
        let at = |px: &[u8], x: usize, y: usize| px[(y * SLOT_W + x) * 4..][..4].to_vec();
        let filled = sprite_pixels(&sprite, &palette, false, (SLOT_W, SLOT_H));
        assert_eq!(filled.len(), SLOT_W * SLOT_H * 4);
        assert_eq!(at(&filled, 8, SLOT_H - 1), vec![0, 200, 180, 255]);
        assert_eq!(filled.chunks(4).filter(|p| p[3] > 0).count(), 1);
        let greyed = sprite_pixels(&sprite, &palette, true, (SLOT_W, SLOT_H));
        assert_eq!(at(&greyed, 8, SLOT_H - 1), vec![138, 138, 138, 150]);
    }

    #[test]
    fn every_kind_but_wildmen_has_its_own_figure() {
        let figures: Vec<_> = UnitKind::ALL.iter().filter_map(|&k| figure_sprite(k)).collect();
        assert_eq!(figures.len(), UNIT_FIGURES.count());
        assert!(figures.iter().all(|f| UNIT_FIGURES.contains(f)));
        assert_eq!(figure_sprite(UnitKind::Wildman), None);
        assert_eq!(figure_sprite(UnitKind::Brave), Some(*UNIT_FIGURES.start()));
    }
}
