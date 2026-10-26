mod store;

use serde::Serialize;
use sm0rt_core::crossword::{generate_tier, Crossword as CoreCrossword, Dir, Tier, WordList};
use sm0rt_core::grade::{generate_graded, Difficulty};
use sm0rt_core::mascot::{derive, Config};
use sm0rt_core::sudoku::{Grid, Puzzle as SudokuPuzzle};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use store::{from_u8, SavedGame, Store};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewWindow, WindowEvent,
};

/// Puzzles generated ahead of time so "New puzzle" is instant.
#[derive(Default)]
struct Pool {
    sudoku: Mutex<HashMap<String, Vec<SudokuPuzzle>>>,
    crosswords: Mutex<HashMap<String, Vec<CoreCrossword>>>,
}

const POOL_TARGET: usize = 2;
const SUDOKU_KEYS: [&str; 4] = ["easy", "medium", "hard", "expert"];
const CROSSWORD_KEYS: [&str; 3] = ["easy", "medium", "hard"];

fn gen_sudoku(d: Difficulty) -> SudokuPuzzle {
    generate_graded(&mut rand::thread_rng(), d, 300).0
}

fn gen_crossword(t: Tier) -> Option<CoreCrossword> {
    generate_tier(word_list(t), t, &mut rand::thread_rng(), 600)
}

/// Keeps every pool topped up, one puzzle at a time, so it never hogs the CPU.
fn spawn_pool_worker(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2)); // let the UI come up first
        loop {
            let state = app.state::<AppState>();
            let mut worked = false;
            for key in SUDOKU_KEYS {
                let low = state.pool.sudoku.lock().unwrap().get(key).map_or(0, |v| v.len()) < POOL_TARGET;
                if low {
                    let p = gen_sudoku(parse_difficulty(key).unwrap());
                    state.pool.sudoku.lock().unwrap().entry(key.to_string()).or_default().push(p);
                    worked = true;
                    thread::sleep(Duration::from_millis(150));
                }
            }
            for key in CROSSWORD_KEYS {
                let low = state.pool.crosswords.lock().unwrap().get(key).map_or(0, |v| v.len()) < POOL_TARGET;
                if low {
                    if let Some(c) = gen_crossword(parse_tier(key).unwrap().0) {
                        state.pool.crosswords.lock().unwrap().entry(key.to_string()).or_default().push(c);
                    }
                    worked = true;
                    thread::sleep(Duration::from_millis(150));
                }
            }
            if !worked {
                thread::sleep(Duration::from_secs(2));
            }
        }
    });
}

struct AppState {
    pool: Pool,
    store: Mutex<Store>,
    puzzles: Mutex<HashMap<u64, (Grid, Difficulty)>>,
    /// Crossword id -> solved letters (row-major, b'#' for blocks) and mascot credit.
    crosswords: Mutex<HashMap<u64, (Vec<u8>, Difficulty)>>,
}

/// Clue table is embedded for now; becomes a bundled resource once it grows.
/// Easy only uses common answers, so it gets its own filtered list.
fn word_list(tier: Tier) -> &'static WordList {
    static COMMON: OnceLock<WordList> = OnceLock::new();
    static ALL: OnceLock<WordList> = OnceLock::new();
    let build = |min| WordList::from_tsv(include_str!("../../data/clues.tsv"), min);
    match tier {
        Tier::Easy => COMMON.get_or_init(|| build(Tier::Easy.min_freq())),
        _ => ALL.get_or_init(|| build(1)),
    }
}

fn parse_tier(s: &str) -> Result<(Tier, Difficulty), String> {
    match s {
        "easy" => Ok((Tier::Easy, Difficulty::Easy)),
        "medium" => Ok((Tier::Medium, Difficulty::Medium)),
        "hard" => Ok((Tier::Hard, Difficulty::Hard)),
        _ => Err(format!("unknown tier {s}")),
    }
}

#[derive(Serialize)]
struct ClueDto {
    number: u32,
    dir: &'static str,
    cells: Vec<usize>,
    clue: String,
}

#[derive(Serialize)]
struct CrosswordDto {
    id: u64,
    tier: String,
    size: usize,
    /// True where the cell is a block.
    blocks: Vec<bool>,
    clues: Vec<ClueDto>,
}

#[tauri::command]
async fn new_crossword(state: tauri::State<'_, AppState>, tier: String) -> Result<CrosswordDto, String> {
    let (t, credit) = parse_tier(&tier)?;
    let pooled = state.pool.crosswords.lock().unwrap().get_mut(&tier).and_then(|v| v.pop());
    let cw = match pooled {
        Some(cw) => cw,
        None => tauri::async_runtime::spawn_blocking(move || gen_crossword(t))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("could not generate a crossword")?,
    };

    // Standard numbering: row-major over cells that start an across or down slot.
    let mut starts: Vec<usize> = cw.slots.iter().map(|s| s.cells[0]).collect();
    starts.sort_unstable();
    starts.dedup();
    let number_of = |cell: usize| starts.iter().position(|&c| c == cell).unwrap() as u32 + 1;

    let wl = word_list(t);
    let mut clues: Vec<ClueDto> = cw
        .slots
        .iter()
        .zip(&cw.entry_ids)
        .map(|(s, &eid)| ClueDto {
            number: number_of(s.cells[0]),
            dir: if s.dir == Dir::Across { "across" } else { "down" },
            cells: s.cells.clone(),
            clue: wl.entries[eid as usize].clue.clone(),
        })
        .collect();
    clues.sort_by_key(|c| (c.dir != "across", c.number));

    let id: u64 = rand::random::<u32>() as u64;
    state.crosswords.lock().unwrap().insert(id, (cw.cells.clone(), credit));
    let dto = CrosswordDto { id, tier, size: cw.size, blocks: cw.cells.iter().map(|&b| b == b'#').collect(), clues };
    state.store.lock().unwrap().set_game(
        "crossword",
        SavedGame {
            dto: serde_json::to_value(&dto).map_err(|e| e.to_string())?,
            solution: cw.cells.clone(),
            difficulty: credit as u8,
            progress: serde_json::Value::Null,
        },
    );
    Ok(dto)
}

#[derive(Serialize)]
struct CrosswordCheckDto {
    wrong: Vec<usize>,
    solved: bool,
}

/// `letters`: row-major, one char per cell; blocks and empty cells may be any non-letter.
#[tauri::command]
fn check_crossword(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: u64,
    letters: String,
    reveal_wrong: bool,
) -> Result<CrosswordCheckDto, String> {
    let mut all = state.crosswords.lock().unwrap();
    let (solution, credit) = all.get(&id).ok_or("unknown crossword")?;
    let credit = *credit;
    let got = letters.to_ascii_uppercase().into_bytes();
    if got.len() != solution.len() {
        return Err("bad grid".into());
    }
    let open = |i: usize| solution[i] != b'#';
    let wrong: Vec<usize> =
        (0..got.len()).filter(|&i| open(i) && got[i].is_ascii_uppercase() && got[i] != solution[i]).collect();
    let solved = (0..got.len()).all(|i| !open(i) || got[i] == solution[i]);
    if solved {
        all.remove(&id);
        let mut store = state.store.lock().unwrap();
        store.record_solve(now_secs(), credit);
        store.clear_game("crossword");
        let _ = app.emit("mascot-updated", ());
    }
    Ok(CrosswordCheckDto { wrong: if reveal_wrong { wrong } else { vec![] }, solved })
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn parse_difficulty(s: &str) -> Result<Difficulty, String> {
    match s {
        "easy" => Ok(Difficulty::Easy),
        "medium" => Ok(Difficulty::Medium),
        "hard" => Ok(Difficulty::Hard),
        "expert" => Ok(Difficulty::Expert),
        _ => Err(format!("unknown difficulty {s}")),
    }
}

#[derive(Serialize)]
struct PuzzleDto {
    id: u64,
    difficulty: String,
    cells: Vec<u8>,
}

#[tauri::command]
async fn new_puzzle(state: tauri::State<'_, AppState>, difficulty: String) -> Result<PuzzleDto, String> {
    let d = parse_difficulty(&difficulty)?;
    let pooled = state.pool.sudoku.lock().unwrap().get_mut(&difficulty).and_then(|v| v.pop());
    let p = match pooled {
        Some(p) => p,
        None => tauri::async_runtime::spawn_blocking(move || gen_sudoku(d)).await.map_err(|e| e.to_string())?,
    };
    let id: u64 = rand::random::<u32>() as u64;
    state.puzzles.lock().unwrap().insert(id, (p.solution, d));
    let dto = PuzzleDto { id, difficulty, cells: p.puzzle.to_vec() };
    state.store.lock().unwrap().set_game(
        "sudoku",
        SavedGame {
            dto: serde_json::to_value(&dto).map_err(|e| e.to_string())?,
            solution: p.solution.to_vec(),
            difficulty: d as u8,
            progress: serde_json::Value::Null,
        },
    );
    Ok(dto)
}

#[derive(Serialize)]
struct LoadedGame {
    dto: serde_json::Value,
    progress: serde_json::Value,
}

/// Restores the unfinished game of `kind` ("sudoku" | "crossword"), re-arming its solution for checking.
#[tauri::command]
fn load_game(state: tauri::State<AppState>, kind: String) -> Option<LoadedGame> {
    let store = state.store.lock().unwrap();
    let g = store.game(&kind)?;
    let id = g.dto.get("id")?.as_u64()?;
    let credit = from_u8(g.difficulty);
    match kind.as_str() {
        "sudoku" => {
            let grid: Grid = g.solution.clone().try_into().ok()?;
            state.puzzles.lock().unwrap().insert(id, (grid, credit));
        }
        "crossword" => {
            state.crosswords.lock().unwrap().insert(id, (g.solution.clone(), credit));
        }
        _ => return None,
    }
    Some(LoadedGame { dto: g.dto.clone(), progress: g.progress.clone() })
}

#[tauri::command]
fn save_progress(state: tauri::State<AppState>, kind: String, id: u64, progress: serde_json::Value) {
    state.store.lock().unwrap().update_progress(&kind, id, progress);
}

#[derive(Serialize)]
struct CheckDto {
    /// Cells that break a row/column/box rule.
    conflicts: Vec<usize>,
    /// Filled cells that differ from the solution (only when `reveal_wrong`).
    wrong: Vec<usize>,
    solved: bool,
}

fn find_conflicts(cells: &[u8]) -> Vec<usize> {
    let peer = |a: usize, b: usize| a / 9 == b / 9 || a % 9 == b % 9 || (a / 27 == b / 27 && a % 9 / 3 == b % 9 / 3);
    (0..81)
        .filter(|&i| cells[i] != 0 && (0..81).any(|j| j != i && cells[j] == cells[i] && peer(i, j)))
        .collect()
}

#[tauri::command]
fn check_puzzle(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: u64,
    cells: Vec<u8>,
    reveal_wrong: bool,
) -> Result<CheckDto, String> {
    if cells.len() != 81 || cells.iter().any(|&c| c > 9) {
        return Err("bad grid".into());
    }
    let mut puzzles = state.puzzles.lock().unwrap();
    let (solution, difficulty) = *puzzles.get(&id).ok_or("unknown puzzle")?;
    let wrong: Vec<usize> = (0..81).filter(|&i| cells[i] != 0 && cells[i] != solution[i]).collect();
    let solved = (0..81).all(|i| cells[i] == solution[i]);
    if solved {
        puzzles.remove(&id);
        let mut store = state.store.lock().unwrap();
        store.record_solve(now_secs(), difficulty);
        store.clear_game("sudoku");
        let _ = app.emit("mascot-updated", ());
    }
    Ok(CheckDto { conflicts: find_conflicts(&cells), wrong: if reveal_wrong { wrong } else { vec![] }, solved })
}

#[derive(Serialize)]
struct MascotDto {
    health: f64,
    stage: &'static str,
    streak: u32,
    freezes: u32,
    last_activity: i64,
}

#[tauri::command]
fn get_mascot(state: tauri::State<AppState>, tz_offset_secs: i64) -> MascotDto {
    use sm0rt_core::mascot::Stage::*;
    let store = state.store.lock().unwrap();
    let m = derive(&Config::default(), store.created_at(), &store.events(), now_secs(), tz_offset_secs);
    MascotDto {
        health: m.health,
        stage: match m.stage {
            Thriving => "thriving",
            Content => "content",
            Bored => "bored",
            Glazed => "glazed",
            BrainRotted => "rotted",
        },
        streak: m.streak,
        freezes: m.freezes,
        last_activity: m.last_activity,
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Opens the main window, optionally switching it to a tab ("sudoku" | "crossword").
#[tauri::command]
fn show_main(app: AppHandle, tab: Option<String>) {
    show_main_window(&app);
    if let Some(tab) = tab {
        let _ = app.emit_to("main", "open-tab", tab);
    }
}

fn set_mascot_visible_inner(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window("mascot") {
        let _ = if visible { w.show() } else { w.hide() };
    }
}

#[tauri::command]
fn set_mascot_visible(app: AppHandle, visible: bool) {
    set_mascot_visible_inner(&app, visible);
}

/// Sprite hit area in logical px, relative to the window's top-left (circle).
/// Window is 240x240; the sprite sits low, leaving the top free for the speech bubble.
const HIT_CX: f64 = 120.0;
const HIT_CY: f64 = 150.0;
const HIT_R: f64 = 60.0;

/// Window is click-through except over the sprite circle (spike S1).
fn spawn_hit_test(win: WebviewWindow) {
    thread::spawn(move || {
        let mut ignoring = None;
        loop {
            thread::sleep(Duration::from_millis(30));
            let (Ok(cur), Ok(pos), Ok(scale)) = (win.cursor_position(), win.outer_position(), win.scale_factor())
            else {
                continue;
            };
            let dx = (cur.x - pos.x as f64) / scale - HIT_CX;
            let dy = (cur.y - pos.y as f64) / scale - HIT_CY;
            let want_ignore = dx * dx + dy * dy > HIT_R * HIT_R;
            if ignoring != Some(want_ignore) {
                let _ = win.set_ignore_cursor_events(want_ignore);
                ignoring = Some(want_ignore);
            }
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            new_puzzle,
            check_puzzle,
            new_crossword,
            check_crossword,
            load_game,
            save_progress,
            get_mascot,
            show_main,
            set_mascot_visible
        ])
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(AppState {
                pool: Pool::default(),
                store: Mutex::new(Store::open(dir, now_secs())),
                puzzles: Mutex::new(HashMap::new()),
                crosswords: Mutex::new(HashMap::new()),
            });

            spawn_hit_test(app.get_webview_window("mascot").expect("mascot window"));
            spawn_pool_worker(app.handle().clone());

            let open = MenuItem::with_id(app, "open", "Open Sm0rt", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "Show / hide mascot", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &toggle, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("icon"))
                .tooltip("Sm0rt")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_main_window(app),
                    "toggle" => {
                        let shown = app.get_webview_window("mascot").and_then(|w| w.is_visible().ok()).unwrap_or(true);
                        set_mascot_visible_inner(app, !shown);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window hides it; the mascot keeps living on the desktop.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error running app");
}
