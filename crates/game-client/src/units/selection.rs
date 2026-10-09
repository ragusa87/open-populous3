//! Unit selection, as in the original: left click on one of your units selects it (Ctrl adds or
//! removes it), left drag draws a box that selects your units inside it (Ctrl adds them), right
//! click clears the selection. The shaman is a unit like the others here; clicking her panel preview
//! selects her alone (`select_only`). Left click on the ground sends
//! the selection there, each unit to a free cell of its own (`GameMap::dispatch`), or round one of
//! the player's camp fires when clicked on it (`ground_click`); on a tree, braves cut it and the others
//! walk next to it (`GameMap::cut_orders`); on a wood pile, braves with empty hands pick up a piece
//! each (`GameMap::pick_orders`); on one of the player's buildings still to build, braves work on it
//! (`GameMap::build_orders`); Shift + right click on one of the player's camp fires puts it out, on
//! a plan not flat yet cancels it, and keeps the selection (`shift_right_click`); P prays, X stops.
//! With Ctrl held, these orders are chained after the units' current ones (`chained`). Only selected units show their health bar, and the
//! cursor shows how many units are selected when more than one. While a spell is aimed the mouse
//! belongs to it (`hud::spells`): clicks neither select nor send units.

use super::{world_units, UnitView, PLAYER};
use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::pick_ground;
use crate::hud::PANEL_WIDTH;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::command::Command;
use game_core::unit::{Order, Unit};

/// Cursor travel (screen pixels) before a press becomes a box drag.
pub const DRAG_PX: f32 = 6.0;
/// A click hits a unit within this many pixels of its feet-to-head line.
pub const HIT_PX: f32 = 10.0;
/// Height of a unit's hit line, in cells (the shaman sprite is ~0.39 cell tall).
pub const UNIT_HEIGHT: f32 = 0.4;
const BOX_BORDER: Color = Color::srgba(1.0, 1.0, 0.95, 0.85);
const BOX_FILL: Color = Color::srgba(1.0, 1.0, 0.95, 0.08);

/// The local player's selected units, by `Unit::id`, in selection order.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Selection {
    pub units: Vec<u32>,
}

/// A unit the player could select, as seen on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnScreen {
    pub id: u32,
    pub feet: Vec2,
    pub head: Vec2,
}

impl Selection {
    pub fn contains(&self, id: u32) -> bool {
        self.units.contains(&id)
    }

    pub fn clear(&mut self) {
        self.units.clear();
    }

    pub fn select_only(&mut self, id: u32) {
        self.units = vec![id];
    }

    /// Click on a unit. `add` (Ctrl) toggles it in the selection instead of replacing it.
    pub fn click(&mut self, id: u32, add: bool) {
        if !add {
            self.select_only(id);
        } else if let Some(i) = self.units.iter().position(|&u| u == id) {
            self.units.remove(i);
        } else {
            self.units.push(id);
        }
    }

    /// Box over `hits`; `add` (Ctrl) keeps the current selection. An empty box clears it unless adding.
    pub fn select_box(&mut self, hits: &[OnScreen], add: bool) {
        if !add {
            self.clear();
        }
        for h in hits {
            if !self.contains(h.id) {
                self.units.push(h.id);
            }
        }
    }

    /// Drops units that can no longer be selected (dead, gone, inside something).
    pub fn retain(&mut self, units: &[Unit]) {
        self.units.retain(|&id| units.iter().any(|u| u.id == id && selectable(u)));
    }

    /// One order per selected unit.
    pub fn commands(&self, order: Order) -> Vec<Command> {
        self.units.iter().map(|&unit| Command::OrderUnit { player: PLAYER, unit, order }).collect()
    }
}

/// The player's own living units. Units inside a vehicle or building will be excluded here once
/// they exist: selecting a vehicle or building does not select the people inside it.
pub fn selectable(u: &Unit) -> bool {
    u.owner == PLAYER && u.is_alive()
}

/// The unit under the cursor: nearest feet-to-head line within `HIT_PX`.
pub fn unit_at(cursor: Vec2, units: &[OnScreen]) -> Option<OnScreen> {
    units
        .iter()
        .map(|u| (segment_distance(cursor, u.feet, u.head), u))
        .filter(|(d, _)| *d <= HIT_PX)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, u)| *u)
}

/// Units whose middle is inside the box between corners `a` and `b`.
pub fn in_box(a: Vec2, b: Vec2, units: &[OnScreen]) -> Vec<OnScreen> {
    let rect = Rect::from_corners(a, b);
    units.iter().filter(|u| rect.contains((u.feet + u.head) / 2.0)).copied().collect()
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// What a left press became once released.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Click(Vec2),
    Box(Vec2, Vec2),
}

/// Left button press, from press to release.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Drag {
    start: Option<Vec2>,
    dragging: bool,
}

impl Drag {
    pub fn press(&mut self, at: Vec2) {
        *self = Drag { start: Some(at), dragging: false };
    }

    /// Box corners while dragging.
    pub fn moved(&mut self, at: Vec2) -> Option<(Vec2, Vec2)> {
        let start = self.start?;
        self.dragging |= start.distance(at) > DRAG_PX;
        self.dragging.then_some((start, at))
    }

    pub fn release(&mut self, at: Vec2) -> Option<Gesture> {
        let gesture = match (self.start, self.moved(at)) {
            (_, Some((a, b))) => Some(Gesture::Box(a, b)),
            (Some(a), None) => Some(Gesture::Click(a)),
            _ => None,
        };
        *self = Drag::default();
        gesture
    }
}

#[derive(Component)]
struct SelectionBox;
#[derive(Component)]
struct CountLabel;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>().init_resource::<Drag>().add_systems(Startup, spawn_overlays).add_systems(Update, draw_overlays.after(select_and_order).in_set(crate::menu::Gameplay));
    }
}

fn spawn_overlays(mut commands: Commands) {
    commands.spawn((
        SelectionBox,
        Node { position_type: PositionType::Absolute, border: UiRect::all(px(1)), ..default() },
        BorderColor::all(BOX_BORDER),
        BackgroundColor(BOX_FILL),
        GlobalZIndex(i32::MAX - 1),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
    commands.spawn((
        CountLabel,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, ..default() },
        GlobalZIndex(i32::MAX),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
}

/// The player's selectable units on screen this frame.
pub fn on_screen(
    map: &game_core::map::GameMap,
    views: &Query<(&UnitView, &GlobalTransform, &Visibility)>,
    cam: (&Camera, &GlobalTransform),
) -> Vec<OnScreen> {
    views
        .iter()
        .filter(|(_, _, vis)| **vis != Visibility::Hidden)
        .filter_map(|(view, gt, _)| {
            let u = map.units.get(view.0).filter(|u| selectable(u) && !super::hidden_inside(map, u))?;
            let feet = cam.0.world_to_viewport(cam.1, gt.translation()).ok()?;
            let head = cam.0.world_to_viewport(cam.1, gt.translation() + gt.up() * UNIT_HEIGHT).ok()?;
            Some(OnScreen { id: u.id, feet, head })
        })
        .collect()
}

/// Left/right mouse and P/X: selection changes and orders to the selection.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_and_order(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    views: Query<(&UnitView, &GlobalTransform, &Visibility)>,
    ui: Query<&Interaction>,
    params: Res<CurveParamsRes>,
    rig: Res<CameraRig>,
    mut map: ResMut<CurrentMap>,
    mut selection: ResMut<Selection>,
    mut drag: ResMut<Drag>,
    spell: Res<crate::hud::spells::SelectedSpell>,
    blueprint: Res<crate::blueprint::Blueprint>,
    tree: Res<crate::nature::HoveredTree>,
) {
    selection.retain(&map.0.units);
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let on_map = cursor.filter(|c| c.x > PANEL_WIDTH && !over_ui && spell.0.is_none() && !blueprint.is_active());
    let add = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let mut moves = Vec::new();
    // Shift + right click removes what is under the cursor (a camp fire) and keeps the selection; a
    // right click on a tree shows its wood (`nature`) and keeps it too.
    if let (true, Some(c)) = (mouse.just_pressed(MouseButton::Right), on_map) {
        let ray = cams.iter().next().and_then(|cam| cam.0.viewport_to_world(cam.1, c).ok());
        let ground = ray.and_then(|r| pick_ground(&map.0.terrain, rig.focus, &params.0, r.origin, *r.direction));
        match ground.and_then(|cell| shift_right_click(&map.0, world_units(cell), shift)) {
            Some(command) => moves.push(command),
            None if tree.tree.is_none() => selection.clear(),
            None => {}
        }
    }
    if let (true, Some(c)) = (mouse.just_pressed(MouseButton::Left), on_map) {
        drag.press(c);
    }
    let gesture = match cursor {
        Some(c) if mouse.just_released(MouseButton::Left) => drag.release(c),
        Some(c) => {
            drag.moved(c);
            None
        }
        None => None,
    };
    let mut orders = Vec::new();
    if let Some(cam) = gesture.and_then(|_| cams.iter().next()) {
        let units = on_screen(&map.0, &views, cam);
        match gesture {
            Some(Gesture::Box(a, b)) => selection.select_box(&in_box(a, b, &units), add),
            Some(Gesture::Click(c)) => match unit_at(c, &units) {
                Some(u) => selection.click(u.id, add),
                None => {
                    let ray = cam.0.viewport_to_world(cam.1, c).ok();
                    let at = ray.and_then(|r| pick_ground(&map.0.terrain, rig.focus, &params.0, r.origin, *r.direction)).map(world_units);
                    // One of the player's buildings wins over a tree in front of or behind it.
                    if let Some(orders) = at.and_then(|at| building_click(&map.0, &selection.units, at)) {
                        moves = orders;
                    } else if let Some(i) = tree.tree {
                        moves = map.0.cut_orders(PLAYER, &selection.units, i);
                    } else if let Some(at) = at {
                        moves = match map.0.wood_at(at) {
                            Some(pile) => map.0.pick_orders(PLAYER, &selection.units, pile),
                            None => ground_click(&map.0, &selection.units, at),
                        };
                    }
                    if add {
                        moves = chained(moves);
                    }
                }
            },
            None => {}
        }
    }
    if keys.just_pressed(KeyCode::KeyP) {
        orders.push(Order::Pray);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        orders.push(Order::Stop);
    }
    let commands = orders.into_iter().flat_map(|order| selection.commands(order));
    let commands: Vec<Command> = if add { chained(commands.collect()) } else { commands.collect() };
    for command in moves.into_iter().chain(commands) {
        map.bypass_change_detection().0.apply(&command);
    }
}

/// Commands for a left click at `at` (world units) on one of the player's buildings with `selected`
/// units: braves work on it while it is still to build; a built hut takes followers in to rest. None
/// elsewhere, or with nothing selected.
pub fn building_click(map: &game_core::map::GameMap, selected: &[u32], at: (u16, u16)) -> Option<Vec<Command>> {
    if selected.is_empty() {
        return None;
    }
    if let Some(site) = map.site_at(PLAYER, at) {
        return Some(map.build_orders(PLAYER, selected, site));
    }
    map.house_at(PLAYER, at).map(|house| map.enter_orders(PLAYER, selected, house))
}

/// Commands for a left click on the ground at `at` (world units) with `selected` units: they go
/// round the player's camp fire there, else walk there.
pub fn ground_click(map: &game_core::map::GameMap, selected: &[u32], at: (u16, u16)) -> Vec<Command> {
    match map.campfire_at(at).filter(|f| f.owner == PLAYER) {
        _ if selected.is_empty() => Vec::new(),
        Some(fire) => map.gather(PLAYER, selected, fire.id),
        None => map.dispatch(PLAYER, selected, at),
    }
}

/// The same orders, chained after each unit's current ones instead of replacing them (Ctrl).
pub fn chained(commands: Vec<Command>) -> Vec<Command> {
    commands
        .into_iter()
        .map(|c| match c {
            Command::OrderUnit { player, unit, order } => Command::QueueOrder { player, unit, order },
            other => other,
        })
        .collect()
}

/// The command of a right click on the ground at `at` (world units) with Shift held (`shift`):
/// putting out the player's camp fire there, else cancelling the player's plan (not flat yet) there.
/// None otherwise (a plain right click deselects).
pub fn shift_right_click(map: &game_core::map::GameMap, at: (u16, u16), shift: bool) -> Option<Command> {
    if !shift {
        return None;
    }
    if let Some(fire) = map.campfire_at(at).filter(|f| f.owner == PLAYER) {
        return Some(Command::RemoveCampfire { player: PLAYER, at: fire.centre() });
    }
    let plan = |b: &&game_core::building::Building| b.owner == PLAYER && b.stage() == game_core::building::Stage::Blueprint && b.covers(at, 0);
    map.buildings.iter().find(plan).map(|_| Command::CancelBuilding { player: PLAYER, at })
}

/// Draws the drag box and the selected count next to the cursor.
#[allow(clippy::type_complexity)]
fn draw_overlays(
    windows: Query<&Window>,
    selection: Res<Selection>,
    mut drag: ResMut<Drag>,
    mut boxes: Query<(&mut Node, &mut Visibility), (With<SelectionBox>, Without<CountLabel>)>,
    mut labels: Query<(&mut Text, &mut Node, &mut Visibility), With<CountLabel>>,
) {
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let corners = cursor.and_then(|c| drag.bypass_change_detection().moved(c));
    for (mut node, mut vis) in &mut boxes {
        match corners.map(|(a, b)| Rect::from_corners(a, b)) {
            Some(r) => {
                (node.left, node.top, node.width, node.height) = (px(r.min.x), px(r.min.y), px(r.width()), px(r.height()));
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    let count = selection.units.len();
    for (mut text, mut node, mut vis) in &mut labels {
        match cursor.filter(|_| count > 1) {
            Some(c) => {
                text.0 = count.to_string();
                (node.left, node.top) = (px(c.x + 18.0), px(c.y + 20.0));
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: u32, x: f32) -> OnScreen {
        OnScreen { id, feet: Vec2::new(x, 100.0), head: Vec2::new(x, 70.0) }
    }

    fn sel(units: &[u32]) -> Selection {
        Selection { units: units.to_vec() }
    }

    #[test]
    fn ground_clicks_on_a_camp_fire() {
        use game_core::unit::Order;
        let map = game_core::map::GameMap::sandbox_buildings();
        let fire = map.campfires[0].centre();
        let brave = map.units.iter().find(|u| u.kind == game_core::unit::UnitKind::Brave && u.campfire().is_none()).unwrap().id;
        assert!(matches!(ground_click(&map, &[brave], fire)[..], [Command::OrderUnit { order: Order::Campfire { .. }, .. }]), "go round it");
        assert!(ground_click(&map, &[], fire).is_empty());
        let away = (fire.0, fire.1.wrapping_add(5 * 512));
        assert!(matches!(ground_click(&map, &[brave], away)[..], [Command::OrderUnit { order: Order::MoveTo { .. }, .. }]));
        assert_eq!(shift_right_click(&map, (fire.0 + 100, fire.1), true), Some(Command::RemoveCampfire { player: PLAYER, at: fire }), "anywhere in its cell");
        assert_eq!(shift_right_click(&map, fire, false), None, "without Shift: deselect");
        assert_eq!(shift_right_click(&map, away, true), None, "no fire there");
    }

    #[test]
    fn ground_clicks_on_a_plan_send_braves_to_build_shift_right_click_cancels_it() {
        let mut map = game_core::map::GameMap::sandbox_buildings();
        let site = map.place_building(PLAYER, game_core::building::BuildingKind::Hut { size: 1 }, (88 * 512, 64 * 512), 0).unwrap();
        let centre = map.buildings[site].centre();
        let brave = map.units.iter().find(|u| u.kind == game_core::unit::UnitKind::Brave && u.campfire().is_none()).unwrap().id;
        assert!(matches!(building_click(&map, &[brave], centre).unwrap()[..], [Command::OrderUnit { order: Order::Build { site: (45056, 32768) }, .. }]));
        assert_eq!(building_click(&map, &[], centre), None, "nothing selected");
        let hut = map.buildings.iter().find(|b| b.owner == PLAYER && b.kind == game_core::building::BuildingKind::Hut { size: 2 } && b.stage() == game_core::building::Stage::Built).unwrap();
        let (hut_site, hut_centre) = ((hut.x, hut.z), hut.centre());
        assert!(matches!(building_click(&map, &[brave], hut_centre).unwrap()[..], [Command::OrderUnit { order: Order::Enter { site }, .. }] if site == hut_site), "a built hut: rest inside");
        assert_eq!(shift_right_click(&map, centre, true), Some(Command::CancelBuilding { player: PLAYER, at: centre }));
        assert_eq!(shift_right_click(&map, centre, false), None);
        map.buildings[site].flat = true;
        assert_eq!(shift_right_click(&map, centre, true), None, "flat: no more a plan");
    }

    #[test]
    fn ctrl_chains_unit_orders_and_keeps_the_others() {
        let cmds = vec![
            Command::OrderUnit { player: PLAYER, unit: 4, order: Order::Pray },
            Command::RemoveCampfire { player: PLAYER, at: (1, 2) },
        ];
        assert_eq!(
            chained(cmds),
            [Command::QueueOrder { player: PLAYER, unit: 4, order: Order::Pray }, Command::RemoveCampfire { player: PLAYER, at: (1, 2) }]
        );
    }

    #[test]
    fn click_replaces_and_ctrl_toggles() {
        let mut s = sel(&[1, 2]);
        s.click(3, false);
        assert_eq!(s.units, [3]);
        s.click(4, true);
        assert_eq!(s.units, [3, 4]);
        s.click(3, true);
        assert_eq!(s.units, [4], "ctrl click on a selected unit removes it");
    }

    #[test]
    fn box_replaces_or_adds_and_empty_clears() {
        let mut s = sel(&[1]);
        s.select_box(&[at(2, 0.0), at(3, 0.0)], false);
        assert_eq!(s.units, [2, 3]);
        s.select_box(&[at(3, 0.0), at(4, 0.0)], true);
        assert_eq!(s.units, [2, 3, 4]);
        s.select_box(&[], true);
        assert_eq!(s.units, [2, 3, 4]);
        s.select_box(&[], false);
        assert!(s.units.is_empty());
        s.select_box(&[at(2, 0.0)], false);
        s.select_only(9);
        assert_eq!(s.units, [9], "the panel preview selects the shaman alone");
    }

    #[test]
    fn keeps_only_the_players_living_units() {
        use game_core::site::ReincarnationSite;
        use game_core::unit::Action;
        let mine = Unit::shaman(1, &ReincarnationSite::at_cell(PLAYER, (1, 1)));
        let theirs = Unit::shaman(2, &ReincarnationSite::at_cell(1, (5, 5)));
        let mut dead = Unit::shaman(3, &ReincarnationSite::at_cell(PLAYER, (1, 1)));
        dead.action = Action::Dead { left: 3 };
        let mut s = sel(&[1, 2, 3, 4]);
        s.retain(&[mine, theirs, dead]);
        assert_eq!(s.units, [1]);
    }

    #[test]
    fn orders_go_to_each_selected_unit() {
        let cmds = sel(&[4, 7]).commands(Order::Stop);
        assert_eq!(cmds, [Command::OrderUnit { player: PLAYER, unit: 4, order: Order::Stop }, Command::OrderUnit { player: PLAYER, unit: 7, order: Order::Stop }]);
    }

    #[test]
    fn picks_the_nearest_unit_line() {
        let units = [at(1, 100.0), at(2, 108.0)];
        assert_eq!(unit_at(Vec2::new(105.0, 85.0), &units).map(|u| u.id), Some(2));
        assert_eq!(unit_at(Vec2::new(98.0, 63.0), &units).map(|u| u.id), Some(1), "just over the head");
        assert_eq!(unit_at(Vec2::new(150.0, 85.0), &units), None);
    }

    #[test]
    fn box_takes_units_whose_middle_is_inside() {
        let units = [at(1, 100.0), at(2, 200.0)];
        let ids = |v: Vec<OnScreen>| v.iter().map(|u| u.id).collect::<Vec<_>>();
        assert_eq!(ids(in_box(Vec2::new(150.0, 50.0), Vec2::new(90.0, 90.0), &units)), [1]);
        assert_eq!(ids(in_box(Vec2::new(0.0, 0.0), Vec2::new(300.0, 80.0), &units)), Vec::<u32>::new(), "middle at y=85");
    }

    #[test]
    fn short_moves_are_clicks_longer_ones_boxes() {
        let mut d = Drag::default();
        assert_eq!(d.release(Vec2::ZERO), None, "release without press");
        d.press(Vec2::new(10.0, 10.0));
        assert_eq!(d.moved(Vec2::new(13.0, 12.0)), None);
        assert_eq!(d.release(Vec2::new(14.0, 10.0)), Some(Gesture::Click(Vec2::new(10.0, 10.0))));
        d.press(Vec2::new(10.0, 10.0));
        assert!(d.moved(Vec2::new(40.0, 40.0)).is_some());
        assert_eq!(d.release(Vec2::new(12.0, 12.0)), Some(Gesture::Box(Vec2::new(10.0, 10.0), Vec2::new(12.0, 12.0))), "stays a box once dragged");
    }
}
