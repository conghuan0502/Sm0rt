//! Mascot state, derived deterministically from a user's solve events and the
//! current time. Nothing here reads a clock: callers pass `now`, so the engine
//! is replayable and conflict-free under sync (see plan: event-sourced state).

use crate::grade::Difficulty;
use std::collections::BTreeSet;

pub const HOUR: i64 = 3600;
pub const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SolveEvent {
    /// Globally unique id; duplicates (from re-sync) are ignored.
    pub id: u64,
    /// Unix seconds.
    pub ts: i64,
    pub difficulty: Difficulty,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub initial_health: f64,
    /// No decay until this long after the last activity.
    pub grace_secs: i64,
    pub decay_per_day: f64,
    /// Health restored per solve, by difficulty (Easy, Medium, Hard, Expert).
    pub gain: [f64; 4],
    pub freeze_every_streak_days: u32,
    pub max_freezes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            initial_health: 80.0,
            grace_secs: 36 * HOUR,
            decay_per_day: 10.0,
            gain: [15.0, 20.0, 25.0, 30.0],
            freeze_every_streak_days: 7,
            max_freezes: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    BrainRotted,
    Glazed,
    Bored,
    Content,
    Thriving,
}

impl Stage {
    pub fn from_health(h: f64) -> Stage {
        match h {
            h if h >= 80.0 => Stage::Thriving,
            h if h >= 60.0 => Stage::Content,
            h if h >= 40.0 => Stage::Bored,
            h if h >= 20.0 => Stage::Glazed,
            _ => Stage::BrainRotted,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MascotState {
    pub health: f64,
    pub stage: Stage,
    /// Streak as of `now` (0 if it has lapsed beyond what a freeze can cover).
    pub streak: u32,
    pub freezes: u32,
    pub last_activity: i64,
    /// Local-day numbers (see `local_day`) with at least one solve.
    pub solved_days: BTreeSet<i64>,
}

/// Local calendar day number for a timestamp and the user's UTC offset.
pub fn local_day(ts: i64, tz_offset_secs: i64) -> i64 {
    (ts + tz_offset_secs).div_euclid(DAY)
}

fn gain_index(d: Difficulty) -> usize {
    d as usize
}

fn decay(cfg: &Config, health: f64, from: i64, to: i64, last_activity: i64) -> f64 {
    let start = from.max(last_activity + cfg.grace_secs);
    if to <= start {
        return health;
    }
    let hours = (to - start) as f64 / HOUR as f64;
    (health - cfg.decay_per_day / 24.0 * hours).max(0.0)
}

/// Replays `events` (any order, duplicates allowed) up to `now`.
/// Events after `now` are ignored (clock-skew safety).
pub fn derive(cfg: &Config, created_at: i64, events: &[SolveEvent], now: i64, tz_offset_secs: i64) -> MascotState {
    let mut evs: Vec<SolveEvent> = events.iter().copied().filter(|e| e.ts <= now).collect();
    evs.sort_by_key(|e| (e.ts, e.id));
    evs.dedup_by_key(|e| e.id);

    let mut health = cfg.initial_health;
    let mut clock = created_at;
    let mut last_activity = created_at;
    let mut streak = 0u32;
    let mut freezes = 0u32;
    let mut last_day: Option<i64> = None;
    let mut solved_days = BTreeSet::new();

    for e in &evs {
        health = decay(cfg, health, clock, e.ts, last_activity);
        clock = e.ts;
        health = (health + cfg.gain[gain_index(e.difficulty)]).min(100.0);
        last_activity = e.ts;

        let day = local_day(e.ts, tz_offset_secs);
        solved_days.insert(day);
        match last_day {
            Some(d) if day == d => {}
            Some(d) if day == d + 1 => streak += 1,
            Some(d) if day == d + 2 && freezes > 0 => {
                freezes -= 1;
                streak += 1;
            }
            _ => streak = 1,
        }
        if last_day != Some(day) && streak > 0 && streak % cfg.freeze_every_streak_days == 0 {
            freezes = (freezes + 1).min(cfg.max_freezes);
        }
        last_day = Some(day);
    }

    health = decay(cfg, health, clock, now, last_activity);

    // A lapsed streak reads as 0 once even a freeze can't bridge the gap.
    let today = local_day(now, tz_offset_secs);
    let live_streak = match last_day {
        Some(d) if today - d <= 1 => streak,
        Some(d) if today - d == 2 && freezes > 0 => streak,
        _ => 0,
    };

    MascotState { health, stage: Stage::from_health(health), streak: live_streak, freezes, last_activity, solved_days }
}

/// Consecutive days (ending today, or yesterday if today isn't done yet) on
/// which both partners solved at least one puzzle.
pub fn couple_streak(a: &MascotState, b: &MascotState, today: i64) -> u32 {
    let both = |d: i64| a.solved_days.contains(&d) && b.solved_days.contains(&d);
    let mut d = if both(today) { today } else { today - 1 };
    let mut n = 0;
    while both(d) {
        n += 1;
        d -= 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const T0: i64 = 1_700_000_000 - 1_700_000_000 % DAY; // a UTC midnight
    const UTC: i64 = 0;

    fn ev(id: u64, ts: i64, d: Difficulty) -> SolveEvent {
        SolveEvent { id, ts, difficulty: d }
    }
    fn st(events: &[SolveEvent], now: i64) -> MascotState {
        derive(&Config::default(), T0, events, now, UTC)
    }

    #[test]
    fn no_decay_inside_grace_then_linear() {
        assert_eq!(st(&[], T0 + 36 * HOUR).health, 80.0);
        let s = st(&[], T0 + 36 * HOUR + DAY);
        assert!((s.health - 70.0).abs() < 1e-9);
        assert_eq!(s.stage, Stage::Content);
    }

    #[test]
    fn long_neglect_floors_at_brain_rotted_not_negative() {
        let s = st(&[], T0 + 60 * DAY);
        assert_eq!(s.health, 0.0);
        assert_eq!(s.stage, Stage::BrainRotted);
    }

    #[test]
    fn one_solve_recovers_roughly_a_stage() {
        let now = T0 + 12 * DAY;
        let before = st(&[], now);
        let after = st(&[ev(1, now, Difficulty::Medium)], now);
        assert!((after.health - (before.health + 20.0)).abs() < 1e-9);
        assert!(after.stage > before.stage);
    }

    #[test]
    fn harder_puzzles_restore_more_and_health_caps_at_100() {
        let now = T0 + DAY;
        let easy = st(&[ev(1, now, Difficulty::Easy)], now).health;
        let hard = st(&[ev(1, now, Difficulty::Hard)], now).health;
        assert!(hard > easy);
        let many: Vec<_> = (0..20).map(|i| ev(i, now, Difficulty::Expert)).collect();
        assert_eq!(st(&many, now).health, 100.0);
    }

    #[test]
    fn solving_resets_the_grace_clock() {
        // 3 days idle: 36h decay past grace = 15 -> 65, solve +15 -> 80.
        let solved_at = T0 + 3 * DAY;
        let s = st(&[ev(1, solved_at, Difficulty::Easy)], solved_at + 35 * HOUR);
        assert!((s.health - 80.0).abs() < 1e-9); // no decay after solving, within grace
        let later = st(&[ev(1, solved_at, Difficulty::Easy)], solved_at + 36 * HOUR + DAY);
        assert!((s.health - later.health - 10.0).abs() < 1e-9);
    }

    #[test]
    fn streak_counts_consecutive_days_and_ignores_same_day_repeats() {
        let evs = [
            ev(1, T0 + 9 * HOUR, Difficulty::Easy),
            ev(2, T0 + 20 * HOUR, Difficulty::Easy),
            ev(3, T0 + DAY + HOUR, Difficulty::Easy),
            ev(4, T0 + 2 * DAY + HOUR, Difficulty::Easy),
        ];
        assert_eq!(st(&evs, T0 + 2 * DAY + 2 * HOUR).streak, 3);
    }

    #[test]
    fn streak_stays_alive_until_end_of_the_next_day_then_lapses() {
        let evs = [ev(1, T0 + HOUR, Difficulty::Easy)];
        assert_eq!(st(&evs, T0 + DAY + 23 * HOUR).streak, 1); // yesterday's solve still counts
        assert_eq!(st(&evs, T0 + 2 * DAY + HOUR).streak, 0);
    }

    #[test]
    fn freeze_is_earned_at_seven_days_and_bridges_one_missed_day() {
        let mut evs: Vec<_> = (0..7).map(|d| ev(d as u64, T0 + d * DAY + HOUR, Difficulty::Easy)).collect();
        let s = st(&evs, T0 + 6 * DAY + 2 * HOUR);
        assert_eq!((s.streak, s.freezes), (7, 1));
        // Skip day 7, solve on day 8: freeze consumed, streak continues.
        evs.push(ev(100, T0 + 8 * DAY + HOUR, Difficulty::Easy));
        let s = st(&evs, T0 + 8 * DAY + 2 * HOUR);
        assert_eq!((s.streak, s.freezes), (8, 0));
        // Skipping two days breaks it.
        evs.push(ev(101, T0 + 11 * DAY + HOUR, Difficulty::Easy));
        assert_eq!(st(&evs, T0 + 11 * DAY + 2 * HOUR).streak, 1);
    }

    #[test]
    fn day_boundaries_follow_the_users_timezone() {
        // 23:30 and 00:30 UTC are different UTC days but the same day at UTC+1 (00:30 and 01:30).
        let a = ev(1, T0 + 23 * HOUR + 30 * 60, Difficulty::Easy);
        let b = ev(2, T0 + DAY + 30 * 60, Difficulty::Easy);
        let utc = derive(&Config::default(), T0, &[a, b], T0 + DAY + HOUR, 0);
        assert_eq!(utc.streak, 2); // two UTC days
        let plus1 = derive(&Config::default(), T0, &[a, b], T0 + DAY + HOUR, HOUR);
        assert_eq!(plus1.streak, 1); // both fall on the same local day
    }

    #[test]
    fn future_events_and_duplicates_are_ignored() {
        let now = T0 + 2 * DAY;
        let evs = [ev(1, now, Difficulty::Easy), ev(1, now, Difficulty::Easy), ev(2, now + DAY, Difficulty::Expert)];
        let s = st(&evs, now);
        assert_eq!(s.streak, 1);
        assert_eq!(s.solved_days.len(), 1);
        let one = st(&[ev(1, now, Difficulty::Easy)], now);
        assert_eq!(s.health, one.health);
    }

    #[test]
    fn couple_streak_needs_both_partners_each_day() {
        let a: Vec<_> = (0..4).map(|d| ev(d as u64, T0 + d * DAY + HOUR, Difficulty::Easy)).collect();
        let b: Vec<_> = (0..4).filter(|d| *d != 1).map(|d| ev(50 + d as u64, T0 + d * DAY + 2 * HOUR, Difficulty::Easy)).collect();
        let now = T0 + 3 * DAY + 3 * HOUR;
        let (sa, sb) = (st(&a, now), st(&b, now));
        assert_eq!(couple_streak(&sa, &sb, local_day(now, UTC)), 2); // days 2 and 3; day 1 missing for B
    }

    fn arb_events() -> impl Strategy<Value = Vec<SolveEvent>> {
        prop::collection::vec((0u64..30, 0i64..(20 * DAY), 0usize..4), 0..30).prop_map(|v| {
            let ds = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert];
            v.into_iter().map(|(id, ts, d)| ev(id, T0 + ts, ds[d])).collect()
        })
    }

    proptest! {
        #[test]
        fn derive_is_independent_of_arrival_order(evs in arb_events(), seed in any::<u64>()) {
            // Dedup by id first so permutations carry identical content per id.
            let mut uniq: Vec<SolveEvent> = Vec::new();
            for e in evs { if !uniq.iter().any(|u| u.id == e.id) { uniq.push(e); } }
            let mut shuffled = uniq.clone();
            let mut x = seed | 1;
            for i in (1..shuffled.len()).rev() {
                x ^= x << 13; x ^= x >> 7; x ^= x << 17;
                shuffled.swap(i, (x % (i as u64 + 1)) as usize);
            }
            let now = T0 + 25 * DAY;
            let a = st(&uniq, now);
            let b = st(&shuffled, now);
            prop_assert_eq!(a.health, b.health);
            prop_assert_eq!(a.streak, b.streak);
            prop_assert_eq!(a.freezes, b.freezes);
            prop_assert!(a.health >= 0.0 && a.health <= 100.0);
        }
    }
}
