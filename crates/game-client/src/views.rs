//! World views: a simulation object (the model, in game-core) is drawn by one view object here, linked
//! by the model's stable id (`GameMap::new_id`), never by its index in a list. Everything about drawing
//! one kind of thing lives in its view type and file (`WorldView`); one generic system, `sync_views`,
//! spawns a view when its model appears, updates it every frame at the drawn moment (`SimNow`) and
//! despawns it once its id is gone. Each view type has its own plugin registering `sync_views::<V>`.
//! A view's poses are pure functions of the drawn moment and a seed from the model's id
//! (`sim_time::seed`), so any client draws the same picture from the simulation state alone.

use crate::sim_time::SimNow;
use crate::units::SimClock;
use crate::world::CurrentMap;
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::prelude::*;
use game_core::map::GameMap;

/// How one kind of model is drawn.
pub trait WorldView: Component + Sized {
    type Model;
    /// What spawning and updating need (assets, queries on the view's parts).
    type Params: SystemParam + 'static;

    /// The models drawn by this kind of view.
    fn models(map: &GameMap) -> impl Iterator<Item = &Self::Model>;
    /// The model's stable id, never given twice on a map.
    fn id(model: &Self::Model) -> u32;
    /// Builds the view on `view` (components, children); None to try again next frame (not ready yet).
    fn spawn(model: &Self::Model, view: &mut EntityCommands, params: &mut SystemParamItem<Self::Params>) -> Option<Self>;
    /// Poses the view at `now`.
    fn update(&self, model: &Self::Model, now: SimNow, params: &mut SystemParamItem<Self::Params>);
}

/// The model a view draws, and the game it was drawn in (`SimClock::game`): ids restart on a new map.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewOf {
    pub id: u32,
    pub game: u32,
}

/// What `sync_views` does this frame, by index into its inputs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ViewSync {
    /// (view, model) pairs drawn on.
    pub keep: Vec<(usize, usize)>,
    /// Views whose model is gone, of another game, or doubled.
    pub despawn: Vec<usize>,
    /// Models without a view.
    pub spawn: Vec<usize>,
}

/// Matches `views` to `models` (ids) by id, in game `game`.
pub fn plan(views: &[ViewOf], game: u32, models: &[u32]) -> ViewSync {
    let mut sorted: Vec<(u32, usize)> = models.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    sorted.sort_unstable();
    let mut drawn = vec![false; models.len()];
    let mut sync = ViewSync::default();
    for (v, of) in views.iter().enumerate() {
        let found = sorted.binary_search_by_key(&of.id, |m| m.0).ok().map(|k| sorted[k].1);
        match found.filter(|&m| of.game == game && !drawn[m]) {
            Some(m) => {
                drawn[m] = true;
                sync.keep.push((v, m));
            }
            None => sync.despawn.push(v),
        }
    }
    sync.spawn = (0..models.len()).filter(|&m| !drawn[m]).collect();
    sync
}

/// Spawns, updates and despawns the views of kind `V` to match the map's models.
pub fn sync_views<V: WorldView>(mut commands: Commands, map: Res<CurrentMap>, now: Res<SimNow>, clock: Res<SimClock>, views: Query<(Entity, &ViewOf, &V)>, mut params: StaticSystemParam<V::Params>) {
    let models: Vec<&V::Model> = V::models(&map.0).collect();
    let ids: Vec<u32> = models.iter().map(|m| V::id(m)).collect();
    let views: Vec<_> = views.iter().collect();
    let of: Vec<ViewOf> = views.iter().map(|v| *v.1).collect();
    let sync = plan(&of, clock.game, &ids);
    for &v in &sync.despawn {
        commands.entity(views[v].0).despawn();
    }
    for &(v, m) in &sync.keep {
        views[v].2.update(models[m], *now, &mut params);
    }
    for m in sync.spawn {
        let mut view = commands.spawn(ViewOf { id: ids[m], game: clock.game });
        match V::spawn(models[m], &mut view, &mut params) {
            Some(v) => {
                view.insert(v);
            }
            None => view.despawn(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use game_core::building::Building;

    #[test]
    fn views_follow_their_models_by_id() {
        let of = |id, game| ViewOf { id, game };
        let s = plan(&[], 0, &[4, 7]);
        assert_eq!((s.keep, s.despawn, s.spawn), (vec![], vec![], vec![0, 1]), "new models get views");
        let s = plan(&[of(4, 0), of(7, 0), of(9, 0)], 0, &[4, 9]);
        assert_eq!((s.keep, s.despawn, s.spawn), (vec![(0, 0), (2, 1)], vec![1], vec![]), "a model gone from the middle: the others keep theirs");
        let s = plan(&[of(4, 0), of(4, 0)], 0, &[4]);
        assert_eq!((s.keep, s.despawn), (vec![(0, 0)], vec![1]), "one view per model");
        let s = plan(&[of(4, 0)], 1, &[4]);
        assert_eq!((s.keep, s.despawn, s.spawn), (vec![], vec![0], vec![0]), "a new game: ids start again");
    }

    /// Draws the busy buildings, and counts its updates.
    #[derive(Component)]
    struct BusyView;

    #[derive(Resource, Default)]
    struct Updates(Vec<u32>);

    impl WorldView for BusyView {
        type Model = Building;
        type Params = ResMut<'static, Updates>;

        fn models(map: &GameMap) -> impl Iterator<Item = &Building> {
            map.buildings.iter().filter(|b| b.inside > 0)
        }

        fn id(b: &Building) -> u32 {
            b.id
        }

        fn spawn(_: &Building, _: &mut EntityCommands, _: &mut ResMut<Updates>) -> Option<Self> {
            Some(BusyView)
        }

        fn update(&self, b: &Building, _: SimNow, updates: &mut ResMut<Updates>) {
            updates.0.push(b.id);
        }
    }

    fn viewed(app: &mut App) -> Vec<(u32, Entity)> {
        let mut v: Vec<_> = app.world_mut().query::<(Entity, &ViewOf, &BusyView)>().iter(app.world()).map(|(e, of, _)| (of.id, e)).collect();
        v.sort();
        v
    }

    #[test]
    fn a_view_comes_and_goes_with_its_model() {
        let mut app = App::new();
        app.init_resource::<SimClock>().init_resource::<SimNow>().init_resource::<Updates>().insert_resource(CurrentMap(GameMap::sandbox_buildings()));
        let sync = |app: &mut App| app.world_mut().run_system_once(sync_views::<BusyView>).unwrap();
        let set_busy = |app: &mut App, i: usize, n: u8| app.world_mut().resource_mut::<CurrentMap>().0.buildings[i].inside = n;
        app.world_mut().resource_mut::<CurrentMap>().0.buildings.iter_mut().for_each(|b| b.inside = 0);
        sync(&mut app);
        assert!(viewed(&mut app).is_empty(), "nobody inside");
        for i in 0..3 {
            set_busy(&mut app, i, 1);
        }
        sync(&mut app);
        let ids: Vec<u32> = app.world().resource::<CurrentMap>().0.buildings[..3].iter().map(|b| b.id).collect();
        let before = viewed(&mut app);
        assert_eq!(before.iter().map(|v| v.0).collect::<Vec<_>>(), ids);
        sync(&mut app);
        assert_eq!(app.world().resource::<Updates>().0, ids, "updated once they exist");
        app.world_mut().resource_mut::<CurrentMap>().0.buildings.remove(1);
        sync(&mut app);
        assert_eq!(viewed(&mut app), vec![before[0], before[2]], "the others keep their views");
        set_busy(&mut app, 0, 0);
        sync(&mut app);
        assert_eq!(viewed(&mut app), vec![before[2]], "gone when idle");
        app.world_mut().resource_mut::<SimClock>().new_game();
        sync(&mut app);
        let after = viewed(&mut app);
        assert_eq!(after.len(), 1);
        assert_ne!(after[0].1, before[2].1, "a new game draws anew");
    }
}
