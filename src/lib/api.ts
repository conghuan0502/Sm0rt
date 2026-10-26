import { invoke } from "@tauri-apps/api/core";

export type Stage = "thriving" | "content" | "bored" | "glazed" | "rotted";
export type Difficulty = "easy" | "medium" | "hard" | "expert";

export interface Mascot {
  health: number;
  stage: Stage;
  streak: number;
  freezes: number;
  last_activity: number;
}

export interface Check {
  conflicts: number[];
  wrong: number[];
  solved: boolean;
}

/** Seconds east of UTC for the user's current timezone. */
const tzOffsetSecs = () => -new Date().getTimezoneOffset() * 60;

const getMascotOnce = () => invoke<Mascot>("get_mascot", { tzOffsetSecs: tzOffsetSecs() });

/** Windows load before Rust finishes `setup`, so the first calls can fail: retry briefly. */
export async function getMascot(): Promise<Mascot> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await getMascotOnce();
    } catch (e) {
      if (attempt >= 10) throw e;
      await new Promise((r) => setTimeout(r, 300));
    }
  }
}
export interface SudokuPuzzle {
  id: number;
  difficulty: Difficulty;
  cells: number[];
}
export type CrosswordTier = "easy" | "medium" | "hard";

export const newPuzzle = (difficulty: Difficulty) => invoke<SudokuPuzzle>("new_puzzle", { difficulty });

/** Restores the unfinished game of a kind, if any. `progress` is whatever the UI last saved. */
export const loadGame = <D, P>(kind: "sudoku" | "crossword") =>
  invoke<{ dto: D; progress: P | null } | null>("load_game", { kind });
export const saveProgress = (kind: "sudoku" | "crossword", id: number, progress: unknown) =>
  invoke<void>("save_progress", { kind, id, progress });

export const fmtTime = (secs: number) =>
  `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
export const checkPuzzle = (id: number, cells: number[], revealWrong: boolean) =>
  invoke<Check>("check_puzzle", { id, cells, revealWrong });
export interface Clue {
  number: number;
  dir: "across" | "down";
  cells: number[];
  clue: string;
}
export interface CrosswordPuzzle {
  id: number;
  tier: CrosswordTier;
  size: number;
  blocks: boolean[];
  clues: Clue[];
}

export const newCrossword = (tier: CrosswordTier) => invoke<CrosswordPuzzle>("new_crossword", { tier });
export const checkCrossword = (id: number, letters: string, revealWrong: boolean) =>
  invoke<{ wrong: number[]; solved: boolean }>("check_crossword", { id, letters, revealWrong });
export type Tab = "sudoku" | "crossword";
export const showMain = (tab?: Tab) => invoke<void>("show_main", { tab });
export const setMascotVisible = (visible: boolean) => invoke<void>("set_mascot_visible", { visible });
