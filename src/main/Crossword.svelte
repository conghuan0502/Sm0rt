<script lang="ts">
  import { onMount } from "svelte";
  import {
    checkCrossword, fmtTime, loadGame, newCrossword, saveProgress,
    type Clue, type CrosswordPuzzle, type CrosswordTier,
  } from "../lib/api";

  let { active }: { active: boolean } = $props();

  let tier = $state<CrosswordTier>("easy");
  let elapsed = $state(0);
  let puzzle = $state<CrosswordPuzzle | null>(null);
  let letters = $state<string[]>([]);
  let selected = $state(0);
  let dir = $state<"across" | "down">("across");
  let wrong = $state(new Set<number>());
  let solved = $state(false);
  let loading = $state(false);
  let error = $state("");

  const size = $derived(puzzle?.size ?? 0);
  const across = $derived(puzzle?.clues.filter((c) => c.dir === "across") ?? []);
  const down = $derived(puzzle?.clues.filter((c) => c.dir === "down") ?? []);

  /** Clue covering `cell` in direction `d`, if any. */
  const clueAt = (cell: number, d: "across" | "down") =>
    puzzle?.clues.find((c) => c.dir === d && c.cells.includes(cell));

  const activeClue = $derived.by(() => {
    return clueAt(selected, dir) ?? clueAt(selected, dir === "across" ? "down" : "across");
  });
  const activeCells = $derived(new Set(activeClue?.cells ?? []));

  /** Number printed in a cell (if it starts a slot). */
  const numbers = $derived.by(() => {
    const m = new Map<number, number>();
    for (const c of puzzle?.clues ?? []) m.set(c.cells[0], c.number);
    return m;
  });

  interface Progress { letters: string[]; selected: number; dir: "across" | "down"; elapsed: number }

  function reset(p: CrosswordPuzzle, progress: Progress | null) {
    puzzle = p; tier = p.tier;
    letters = progress?.letters ?? Array(p.size * p.size).fill("");
    selected = progress?.selected ?? p.blocks.findIndex((b) => !b);
    dir = progress?.dir ?? "across";
    elapsed = progress?.elapsed ?? 0;
    wrong = new Set(); solved = false;
  }

  function persist() {
    if (!puzzle || solved) return;
    saveProgress("crossword", puzzle.id, { letters, selected, dir, elapsed } satisfies Progress).catch(() => {});
  }

  async function start() {
    loading = true; error = "";
    try {
      reset(await newCrossword(tier), null);
    } catch (e) { error = String(e); }
    loading = false;
  }

  async function resume() {
    try {
      const g = await loadGame<CrosswordPuzzle, Progress>("crossword");
      if (g) { reset(g.dto, g.progress); return; }
    } catch (e) { console.error(e); }
    start();
  }

  // Timer: counts only while this tab and window are visible and the puzzle is open.
  $effect(() => {
    if (!active || solved || !puzzle) return;
    const t = setInterval(() => {
      if (document.visibilityState !== "visible") return;
      elapsed += 1;
      if (elapsed % 10 === 0) persist();
    }, 1000);
    return () => clearInterval(t);
  });

  async function verify(reveal: boolean) {
    if (!puzzle || solved) return;
    try {
      const r = await checkCrossword(puzzle.id, letters.map((l) => l || " ").join(""), reveal);
      wrong = new Set(r.wrong);
      if (r.solved) solved = true; // Rust emits "mascot-updated"
    } catch (e) { error = String(e); }
  }

  const open = (i: number) => i >= 0 && i < size * size && !puzzle!.blocks[i];

  /** Step to the next open cell in a direction, staying in the row/column; unchanged at edges. */
  function step(from: number, dr: number, dc: number): number {
    let r = Math.floor(from / size) + dr, c = (from % size) + dc;
    while (r >= 0 && r < size && c >= 0 && c < size) {
      if (open(r * size + c)) return r * size + c;
      r += dr; c += dc;
    }
    return from;
  }

  function jumpClue(delta: number) {
    if (!puzzle) return;
    const order: Clue[] = [...across, ...down];
    const i = order.findIndex((c) => c === activeClue);
    const next = order[(i + delta + order.length) % order.length];
    dir = next.dir;
    selected = next.cells.find((c) => !letters[c]) ?? next.cells[0];
  }

  function type(ch: string) {
    letters[selected] = ch;
    wrong = new Set();
    const cells = activeClue?.cells ?? [];
    const k = cells.indexOf(selected);
    if (k >= 0 && k < cells.length - 1) selected = cells[k + 1];
    persist();
    verify(false);
  }

  function backspace() {
    if (letters[selected]) { letters[selected] = ""; wrong = new Set(); persist(); return; }
    const cells = activeClue?.cells ?? [];
    const k = cells.indexOf(selected);
    if (k > 0) { selected = cells[k - 1]; letters[selected] = ""; wrong = new Set(); persist(); }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!active || !puzzle || solved || e.ctrlKey || e.metaKey || e.altKey) return;
    if (/^[a-zA-Z]$/.test(e.key)) type(e.key.toUpperCase());
    else if (e.key === "Backspace" || e.key === "Delete") backspace();
    else if (e.key === "Tab") jumpClue(e.shiftKey ? -1 : 1);
    else if (e.key === "Enter" || e.key === " ") dir = dir === "across" ? "down" : "across";
    else if (e.key === "ArrowRight") { dir = "across"; selected = step(selected, 0, 1); }
    else if (e.key === "ArrowLeft") { dir = "across"; selected = step(selected, 0, -1); }
    else if (e.key === "ArrowDown") { dir = "down"; selected = step(selected, 1, 0); }
    else if (e.key === "ArrowUp") { dir = "down"; selected = step(selected, -1, 0); }
    else return;
    e.preventDefault();
  }

  function clickCell(i: number) {
    if (!open(i)) return;
    if (i === selected) dir = dir === "across" ? "down" : "across";
    selected = i;
  }

  function pickClue(c: Clue) {
    dir = c.dir;
    selected = c.cells.find((x) => !letters[x]) ?? c.cells[0];
  }

  onMount(resume);
</script>

<svelte:window {onkeydown} onpagehide={persist} />
<svelte:document onvisibilitychange={() => document.visibilityState === "hidden" && persist()} />

{#snippet clueList(title: string, list: Clue[])}
  <h3>{title}</h3>
  <ol>
    {#each list as c}
      <li>
        <button class="clue" class:on={c === activeClue} onclick={() => pickClue(c)}>
          <b>{c.number}</b> {c.clue} <span class="len">({c.cells.length})</span>
        </button>
      </li>
    {/each}
  </ol>
{/snippet}

<section>
  <div class="controls">
    <select bind:value={tier} disabled={loading} aria-label="Difficulty">
      <option value="easy">Easy · 5×5 common words</option>
      <option value="medium">Medium · 5×5</option>
      <option value="hard">Hard · 9×9</option>
    </select>
    <button onclick={start} disabled={loading}>{loading ? "Generating…" : "New crossword"}</button>
    <button onclick={() => verify(true)} disabled={loading || solved}>Check</button>
    <span class="timer" aria-label="Elapsed time">{fmtTime(elapsed)}</span>
  </div>

  {#if solved}<p class="win">Solved in {fmtTime(elapsed)}! Your mascot feels better already.</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}

  {#if puzzle}
    <div class="layout">
      <div class="grid" class:big={size > 7} style={`grid-template-columns: repeat(${size}, 1fr)`} role="grid" aria-label="Crossword grid">
        {#each puzzle.blocks as block, i}
          {#if block}
            <div class="cell block"></div>
          {:else}
            <button
              class="cell"
              class:sel={i === selected}
              class:inword={activeCells.has(i) && i !== selected}
              class:wrong={wrong.has(i)}
              onclick={() => clickCell(i)}
              aria-label={`row ${Math.floor(i / size) + 1} column ${(i % size) + 1} ${letters[i] || "empty"}`}
            >
              {#if numbers.has(i)}<span class="num">{numbers.get(i)}</span>{/if}
              {letters[i]}
            </button>
          {/if}
        {/each}
      </div>

      <div class="clues">
        {#if activeClue}
          <p class="current"><b>{activeClue.number} {activeClue.dir}</b> — {activeClue.clue} ({activeClue.cells.length})</p>
        {/if}
        {@render clueList("Across", across)}
        {@render clueList("Down", down)}
      </div>
    </div>
    <p class="hint">Type letters · arrows move · Space/Enter flips direction · Tab next clue</p>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; align-items: center; gap: 14px; width: 100%; }
  .layout { display: flex; gap: 24px; flex-wrap: wrap; justify-content: center; align-items: flex-start; width: min(820px, 94vw); }
  .grid { display: grid; width: min(340px, 90vw); aspect-ratio: 1; border: 2px solid var(--line-strong); background: var(--line-strong); gap: 1px; }
  .grid.big { width: min(450px, 90vw); }
  .grid.big .cell { font-size: clamp(14px, 3vw, 20px); }
  .timer { font-variant-numeric: tabular-nums; align-self: center; min-width: 48px; text-align: center; opacity: 0.8; }
  .cell { position: relative; background: var(--cell); border: 0; padding: 0; border-radius: 0; color: var(--user);
          font: 600 clamp(18px, 4vw, 26px) system-ui, sans-serif; cursor: pointer; }
  .cell.block { background: var(--line-strong); cursor: default; }
  .cell.inword { background: var(--related); }
  .cell.sel { background: var(--sel); }
  .cell.wrong { background: var(--bad-bg); color: var(--bad); }
  .num { position: absolute; left: 3px; top: 1px; font: 500 10px system-ui, sans-serif; color: var(--fg); }
  .clues { flex: 1; min-width: 240px; font-size: 14px; }
  .current { margin: 0 0 8px; padding: 8px 10px; border-radius: 8px; background: var(--related); }
  h3 { margin: 10px 0 4px; font-size: 14px; }
  ol { list-style: none; margin: 0; padding: 0; }
  .clue { display: block; width: 100%; text-align: left; border: 0; background: transparent; padding: 3px 6px; border-radius: 6px; }
  .clue.on { background: var(--sel); }
  .len { opacity: 0.6; }
  .hint { opacity: 0.6; font-size: 12px; margin: 0; }
</style>
