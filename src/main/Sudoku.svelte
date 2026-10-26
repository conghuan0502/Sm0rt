<script lang="ts">
  import { onMount } from "svelte";
  import Board from "./Board.svelte";
  import {
    checkPuzzle, fmtTime, loadGame, newPuzzle, saveProgress,
    type Difficulty, type SudokuPuzzle,
  } from "../lib/api";

  let { active }: { active: boolean } = $props();

  interface Snapshot { cells: number[]; notes: number[] }
  interface Progress { cells: number[]; notes: number[]; elapsed: number }

  let difficulty = $state<Difficulty>("easy");
  let puzzleId = $state<number | null>(null);
  let given = $state<number[]>(Array(81).fill(0));
  let cells = $state<number[]>(Array(81).fill(0));
  let notes = $state<number[]>(Array(81).fill(0));
  let history = $state<Snapshot[]>([]);
  let noteMode = $state(false);
  let selected = $state<number | null>(null);
  let conflicts = $state(new Set<number>());
  let wrong = $state(new Set<number>());
  let solved = $state(false);
  let loading = $state(false);
  let error = $state("");
  let elapsed = $state(0);

  /** Digits already placed 9 times on the board have nothing left to place. */
  const used = $derived.by(() => {
    const n = Array(10).fill(0);
    for (const v of cells) n[v]++;
    return n;
  });
  const digitDone = (d: number) => used[d] >= 9;

  const peers = (a: number, b: number) =>
    Math.floor(a / 9) === Math.floor(b / 9) ||
    a % 9 === b % 9 ||
    (Math.floor(a / 27) === Math.floor(b / 27) && Math.floor((a % 9) / 3) === Math.floor((b % 9) / 3));

  function reset(p: SudokuPuzzle, progress: Progress | null) {
    puzzleId = p.id; difficulty = p.difficulty; given = p.cells;
    cells = progress?.cells ?? [...p.cells];
    notes = progress?.notes ?? Array(81).fill(0);
    elapsed = progress?.elapsed ?? 0;
    history = []; conflicts = new Set(); wrong = new Set(); solved = false;
    selected = cells.findIndex((c) => c === 0);
  }

  function persist() {
    if (puzzleId === null || solved) return;
    saveProgress("sudoku", puzzleId, { cells, notes, elapsed } satisfies Progress).catch(() => {});
  }

  async function start() {
    loading = true; error = "";
    try {
      reset(await newPuzzle(difficulty), null);
    } catch (e) { error = String(e); }
    loading = false;
  }

  async function resume() {
    try {
      const g = await loadGame<SudokuPuzzle, Progress>("sudoku");
      if (g) {
        reset(g.dto, g.progress);
        verify(false);
        return;
      }
    } catch (e) { console.error(e); }
    start();
  }

  async function verify(reveal: boolean) {
    if (puzzleId === null || solved) return;
    try {
      const r = await checkPuzzle(puzzleId, cells, reveal);
      conflicts = new Set(r.conflicts);
      wrong = new Set(r.wrong);
      if (r.solved) solved = true; // Rust emits "mascot-updated" and clears the saved game
    } catch (e) { error = String(e); }
  }

  const snapshot = () => history.push({ cells: [...cells], notes: [...notes] });

  function setDigit(d: number) {
    if (selected === null || solved || given[selected] !== 0) return;
    if (d !== 0 && digitDone(d) && cells[selected] !== d) return;
    const i = selected;
    if (d !== 0 && noteMode) {
      if (cells[i] !== 0) return; // notes only go in empty cells
      snapshot();
      notes[i] ^= 1 << (d - 1);
      persist();
      return;
    }
    if (d === 0 && cells[i] === 0 && notes[i] === 0) return;
    snapshot();
    if (d === 0) {
      if (cells[i] !== 0) cells[i] = 0; else notes[i] = 0; // erase value first, then notes
    } else {
      cells[i] = d;
      notes[i] = 0;
      // Placing a digit removes it from the pencil marks of its peers.
      for (let j = 0; j < 81; j++) if (peers(i, j)) notes[j] &= ~(1 << (d - 1));
    }
    wrong = new Set();
    persist();
    verify(false);
  }

  function undo() {
    const s = history.pop();
    if (!s || solved) return;
    cells = s.cells; notes = s.notes; wrong = new Set();
    persist();
    verify(false);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!active) return;
    if (e.ctrlKey || e.metaKey) {
      if (e.key.toLowerCase() === "z") { undo(); e.preventDefault(); }
      return;
    }
    if (e.key >= "1" && e.key <= "9") setDigit(Number(e.key));
    else if (e.key === "Backspace" || e.key === "Delete" || e.key === "0") setDigit(0);
    else if (e.key === "n" || e.key === "N") noteMode = !noteMode;
    else if (selected !== null) {
      const r = Math.floor(selected / 9), c = selected % 9;
      if (e.key === "ArrowUp") selected = ((r + 8) % 9) * 9 + c;
      else if (e.key === "ArrowDown") selected = ((r + 1) % 9) * 9 + c;
      else if (e.key === "ArrowLeft") selected = r * 9 + ((c + 8) % 9);
      else if (e.key === "ArrowRight") selected = r * 9 + ((c + 1) % 9);
      else return;
      e.preventDefault();
    }
  }

  // Timer: counts only while this tab and window are visible and the puzzle is open.
  $effect(() => {
    if (!active || solved || puzzleId === null) return;
    const t = setInterval(() => {
      if (document.visibilityState !== "visible") return;
      elapsed += 1;
      if (elapsed % 10 === 0) persist();
    }, 1000);
    return () => clearInterval(t);
  });

  onMount(resume);
</script>

<svelte:window {onkeydown} onpagehide={persist} />
<svelte:document onvisibilitychange={() => document.visibilityState === "hidden" && persist()} />

<section>
  <div class="controls">
    <select bind:value={difficulty} disabled={loading} aria-label="Difficulty">
      <option value="easy">Easy</option><option value="medium">Medium</option>
      <option value="hard">Hard</option><option value="expert">Expert</option>
    </select>
    <button onclick={start} disabled={loading}>{loading ? "Generating…" : "New puzzle"}</button>
    <button onclick={() => verify(true)} disabled={loading || solved}>Check</button>
    <button onclick={undo} disabled={history.length === 0 || solved} title="Ctrl+Z">Undo</button>
    <button class:on={noteMode} aria-pressed={noteMode} onclick={() => (noteMode = !noteMode)} title="Key: N">✎ Notes {noteMode ? "on" : "off"}</button>
    <span class="timer" aria-label="Elapsed time">{fmtTime(elapsed)}</span>
  </div>

  {#if solved}<p class="win">Solved in {fmtTime(elapsed)}! Your mascot feels better already.</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}

  <Board {given} {cells} {notes} {selected} {conflicts} {wrong} onselect={(i) => (selected = i)} />

  <div class="pad">
    {#each [1, 2, 3, 4, 5, 6, 7, 8, 9] as d}
      <button onclick={() => setDigit(d)} disabled={solved || digitDone(d)}>{d}</button>
    {/each}
    <button onclick={() => setDigit(0)} aria-label="Erase">⌫</button>
  </div>
</section>

<style>
  .timer { font-variant-numeric: tabular-nums; align-self: center; min-width: 48px; text-align: center; opacity: 0.8; }
  .on { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
