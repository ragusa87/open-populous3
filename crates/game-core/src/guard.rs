//! Guard the shaman (`G`, pop3-rev-analysis.md "Guard the shaman"): a selected follower's own continuous escort
//! order (`Order::Guard`, `Action::Guarding`, `GameMap::go_guard`/`tend_guards`). She is not protected or
//! untargetable while guarded - live-tested and disproven in the trace: a guarded shaman joins the same fight
//! ring as her guards instead of being shielded from it, the ordinary auto-attack/melee-join rule already covers
//! the observed "guards take the hit" effect without a dedicated protection mechanic.

/// How far a guard may stray from the shaman's current position, per axis, before it walks back towards her
/// [exe, confirmed].
pub const GUARD_RANGE: i32 = 824;
