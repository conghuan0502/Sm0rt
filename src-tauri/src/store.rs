//! Local persistence: a JSON event log in the app data dir.
//! Isolated behind `Store` so it can move to SQLite (see plan) without touching commands.

use serde::{Deserialize, Serialize};
use sm0rt_core::grade::Difficulty;
use sm0rt_core::mascot::SolveEvent;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Serialize, Deserialize, Clone)]
struct StoredEvent {
    id: u64,
    ts: i64,
    difficulty: u8,
}

/// An unfinished puzzle, restorable after restart.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedGame {
    /// What the frontend was sent when the puzzle was created.
    pub dto: serde_json::Value,
    /// Solved cells (digits, or letters with b'#' for blocks); never leaves Rust.
    pub solution: Vec<u8>,
    pub difficulty: u8,
    /// Opaque frontend progress (entries, notes, undo-free; elapsed time).
    pub progress: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct Data {
    created_at: i64,
    events: Vec<StoredEvent>,
    #[serde(default)]
    games: HashMap<String, SavedGame>,
}

pub struct Store {
    path: PathBuf,
    data: Data,
}

pub fn from_u8(v: u8) -> Difficulty {
    match v {
        0 => Difficulty::Easy,
        1 => Difficulty::Medium,
        2 => Difficulty::Hard,
        _ => Difficulty::Expert,
    }
}

impl Store {
    pub fn open(dir: PathBuf, now: i64) -> Store {
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Data { created_at: now, events: Vec::new(), games: HashMap::new() });
        let s = Store { path, data };
        s.save();
        s
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let tmp = self.path.with_extension("tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            }
        }
    }

    pub fn created_at(&self) -> i64 {
        self.data.created_at
    }

    pub fn events(&self) -> Vec<SolveEvent> {
        self.data
            .events
            .iter()
            .map(|e| SolveEvent { id: e.id, ts: e.ts, difficulty: from_u8(e.difficulty) })
            .collect()
    }

    pub fn game(&self, kind: &str) -> Option<&SavedGame> {
        self.data.games.get(kind)
    }

    pub fn set_game(&mut self, kind: &str, game: SavedGame) {
        self.data.games.insert(kind.to_string(), game);
        self.save();
    }

    /// Updates progress only if `id` is still the current game of that kind.
    pub fn update_progress(&mut self, kind: &str, id: u64, progress: serde_json::Value) {
        if let Some(g) = self.data.games.get_mut(kind) {
            if g.dto.get("id").and_then(|v| v.as_u64()) == Some(id) {
                g.progress = progress;
                self.save();
            }
        }
    }

    pub fn clear_game(&mut self, kind: &str) {
        if self.data.games.remove(kind).is_some() {
            self.save();
        }
    }

    pub fn record_solve(&mut self, ts: i64, difficulty: Difficulty) {
        self.data.events.push(StoredEvent { id: rand::random(), ts, difficulty: difficulty as u8 });
        self.save();
    }
}
