//! Game moments turned into sounds (sound.md "Voices"): the selected people bowing, the ordered ones answering, a
//! flyby starting or cancelled, a reincarnation site's stones rising. Pure choices, thin systems.

use crate::audio::{PlaySound, SoundCue};
use crate::units::selection::Selection;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::ai_script::ActiveFlyby;
use game_core::time::Tick;
use game_core::tribes::TribeId;
use std::collections::BTreeMap;
use game_core::unit::{UnitId, UnitKind};

/// The player gave an order to the selection.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct Ordered;

#[derive(Default)]
struct Crowd {
    followers: usize,
    spies: usize,
    preachers: usize,
    shaman: bool,
}

fn crowd(kinds: &[UnitKind]) -> Crowd {
    let mut c = Crowd::default();
    for k in kinds {
        match k {
            UnitKind::Spy => c.spies += 1,
            UnitKind::Preacher => c.preachers += 1,
            UnitKind::Shaman => c.shaman = true,
            UnitKind::Brave | UnitKind::Warrior | UnitKind::Firewarrior => c.followers += 1,
            UnitKind::Wildman | UnitKind::AngelOfDeath => {}
        }
    }
    c
}

/// What a fresh selection says, layered: spies, preachers, the shaman, other followers, each by its crowd size.
pub fn select_lines(kinds: &[UnitKind]) -> Vec<SoundCue> {
    let c = crowd(kinds);
    let mut lines = Vec::new();
    if c.spies > 0 {
        lines.push(SoundCue::SpyBow(c.spies.min(2) as u8));
    }
    if c.preachers > 0 {
        lines.push(SoundCue::PreacherBow);
    }
    if c.shaman {
        lines.push(SoundCue::ShamanSelect);
    }
    if c.followers > 0 {
        lines.push(SoundCue::FollowerBow(c.followers.min(3) as u8));
    }
    lines
}

/// What an ordered selection answers: the shaman's yes, a generic ok for followers, spies and preachers by count.
pub fn order_lines(kinds: &[UnitKind]) -> Vec<SoundCue> {
    let c = crowd(kinds);
    let mut lines = Vec::new();
    if c.shaman {
        lines.push(SoundCue::ShamanYes);
    }
    if c.followers > 0 {
        lines.push(SoundCue::FollowerOk);
    }
    if c.spies > 0 {
        lines.push(SoundCue::SpyOk(c.spies.min(2) as u8));
    }
    if c.preachers > 0 {
        lines.push(SoundCue::PreacherOk);
    }
    lines
}

/// A selection that took in someone new (not one only losing its dead).
pub fn newly_selected(before: &[UnitId], now: &[UnitId]) -> bool {
    now.iter().any(|id| !before.contains(id))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlybyPhase {
    #[default]
    None,
    Playing,
    Cancelled,
}

pub fn flyby_phase(active: Option<&ActiveFlyby>) -> FlybyPhase {
    match active {
        None => FlybyPhase::None,
        Some(a) if a.interrupted_at.is_some() => FlybyPhase::Cancelled,
        Some(_) => FlybyPhase::Playing,
    }
}

/// The sound of a flyby going from `before` to `now`.
pub fn flyby_cue(before: FlybyPhase, now: FlybyPhase) -> Option<SoundCue> {
    match (before, now) {
        (FlybyPhase::None, FlybyPhase::Playing) => Some(SoundCue::FlybyStart),
        (FlybyPhase::Playing, FlybyPhase::Cancelled) => Some(SoundCue::FlybyCancel),
        _ => None,
    }
}

/// Pillar slots of a site built since `before` (each slot's build tick, `ReincarnationSite::pillar_built`).
pub fn risen_slots(before: &[Option<Tick>], now: &[Option<Tick>]) -> Vec<u8> {
    now.iter().zip(before).enumerate().filter(|(_, (n, b))| n.is_some() && b.is_none()).map(|(i, _)| i as u8).collect()
}

/// Which risen stones sound: every second one, counted across every site ("Pillars" [exe]: a global counter's odd
/// values; that the first one counts as odd is ours). `count` goes on from one call to the next.
pub fn sounding(count: &mut u32, risen: usize) -> Vec<bool> {
    (0..risen)
        .map(|_| {
            *count += 1;
            *count % 2 == 1
        })
        .collect()
}

#[derive(Default)]
struct StoneWatch {
    built: BTreeMap<TribeId, Vec<Option<Tick>>>,
    count: u32,
}

fn stone_sounds(map: Res<CurrentMap>, mut watch: Local<StoneWatch>, mut sounds: MessageWriter<PlaySound>) {
    let watch = &mut *watch;
    for site in &map.0.sites {
        let now: Vec<Option<Tick>> = (0..8).map(|slot| site.pillar_built(slot)).collect();
        let before = watch.built.insert(site.owner, now.clone());
        let Some(before) = before else { continue };
        let risen = risen_slots(&before, &now);
        let parts = crate::sites::site_parts(site);
        for (slot, sound) in risen.iter().zip(sounding(&mut watch.count, risen.len())) {
            if let (true, Some(part)) = (sound, parts.get(*slot as usize)) {
                sounds.write(PlaySound::at(SoundCue::StoneRise, part.ground.at));
            }
        }
    }
}

fn kinds(map: &CurrentMap, ids: &[UnitId]) -> Vec<UnitKind> {
    ids.iter().filter_map(|&id| map.0.unit(id)).map(|u| u.kind).collect()
}

fn selection_voices(selection: Res<Selection>, map: Res<CurrentMap>, mut before: Local<Vec<UnitId>>, mut sounds: MessageWriter<PlaySound>) {
    if !selection.is_changed() {
        return;
    }
    if newly_selected(&before, &selection.units) {
        sounds.write_batch(select_lines(&kinds(&map, &selection.units)).into_iter().map(PlaySound::flat));
    }
    before.clone_from(&selection.units);
}

fn order_voices(mut orders: MessageReader<Ordered>, selection: Res<Selection>, map: Res<CurrentMap>, mut sounds: MessageWriter<PlaySound>) {
    if orders.read().count() > 0 {
        sounds.write_batch(order_lines(&kinds(&map, &selection.units)).into_iter().map(PlaySound::flat));
    }
}

fn flyby_sounds(map: Res<CurrentMap>, mut before: Local<FlybyPhase>, mut sounds: MessageWriter<PlaySound>) {
    let now = flyby_phase(map.0.active_flyby.as_ref());
    if let Some(cue) = flyby_cue(*before, now) {
        sounds.write(PlaySound::flat(cue));
    }
    *before = now;
}

pub struct AudioCuesPlugin;

impl Plugin for AudioCuesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Ordered>().add_systems(Update, (selection_voices, order_voices, flyby_sounds, stone_sounds).in_set(crate::menu::Gameplay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::id::Id;
    use UnitKind::*;

    #[test]
    fn one_unit_bows_with_its_own_line() {
        assert_eq!(select_lines(&[Brave]), vec![SoundCue::FollowerBow(1)]);
        assert_eq!(select_lines(&[Spy]), vec![SoundCue::SpyBow(1)]);
        assert_eq!(select_lines(&[Preacher]), vec![SoundCue::PreacherBow]);
        assert_eq!(select_lines(&[Shaman]), vec![SoundCue::ShamanSelect]);
    }

    #[test]
    fn a_group_layers_one_line_per_kind_by_crowd_size() {
        assert_eq!(select_lines(&[Brave, Warrior]), vec![SoundCue::FollowerBow(2)]);
        assert_eq!(select_lines(&[Brave, Warrior, Firewarrior, Brave, Brave]), vec![SoundCue::FollowerBow(3)]);
        assert_eq!(
            select_lines(&[Brave, Spy, Spy, Spy, Shaman, Preacher]),
            vec![SoundCue::SpyBow(2), SoundCue::PreacherBow, SoundCue::ShamanSelect, SoundCue::FollowerBow(1)]
        );
        assert!(select_lines(&[]).is_empty());
    }

    #[test]
    fn an_order_gets_the_shaman_s_yes_and_the_others_ok() {
        assert_eq!(order_lines(&[Shaman, Brave, Warrior, Spy, Preacher]), vec![SoundCue::ShamanYes, SoundCue::FollowerOk, SoundCue::SpyOk(1), SoundCue::PreacherOk]);
        assert_eq!(order_lines(&[Spy, Spy]), vec![SoundCue::SpyOk(2)]);
    }

    #[test]
    fn only_someone_new_in_the_selection_makes_it_bow() {
        let (a, b) = (Id::from_raw(1), Id::from_raw(2));
        assert!(newly_selected(&[], &[a]));
        assert!(newly_selected(&[a], &[a, b]));
        assert!(!newly_selected(&[a, b], &[a]), "one died");
        assert!(!newly_selected(&[a], &[]));
    }

    #[test]
    fn a_stone_sounds_when_its_slot_gets_built() {
        let t = Some(Tick::ZERO);
        assert_eq!(risen_slots(&[None, None, t], &[t, None, t]), vec![0]);
        assert!(risen_slots(&[t, t], &[t, t]).is_empty());
    }

    #[test]
    fn every_second_stone_sounds_across_calls() {
        let mut count = 0;
        assert_eq!(sounding(&mut count, 3), vec![true, false, true]);
        assert_eq!(sounding(&mut count, 2), vec![false, true]);
    }

    #[test]
    fn a_flyby_sounds_when_it_starts_and_when_cancelled() {
        use FlybyPhase::*;
        assert_eq!(flyby_cue(None, Playing), Some(SoundCue::FlybyStart));
        assert_eq!(flyby_cue(Playing, Cancelled), Some(SoundCue::FlybyCancel));
        assert_eq!(flyby_cue(Playing, None), Option::None, "ending on its own is silent");
        assert_eq!(flyby_cue(Playing, Playing), Option::None);
    }
}
