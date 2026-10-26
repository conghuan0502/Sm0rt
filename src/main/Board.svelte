<script lang="ts">
  interface Props {
    given: number[];
    cells: number[];
    /** Pencil marks: bit (d-1) set means digit d is noted. */
    notes: number[];
    selected: number | null;
    conflicts: Set<number>;
    wrong: Set<number>;
    onselect: (i: number) => void;
  }
  let { given, cells, notes, selected, conflicts, wrong, onselect }: Props = $props();

  const peer = (a: number, b: number) =>
    Math.floor(a / 9) === Math.floor(b / 9) ||
    a % 9 === b % 9 ||
    (Math.floor(a / 27) === Math.floor(b / 27) && Math.floor((a % 9) / 3) === Math.floor((b % 9) / 3));
</script>

<div class="board" role="grid" aria-label="Sudoku board">
  {#each cells as v, i}
    {@const sel = selected === i}
    {@const related = selected !== null && !sel && (peer(selected, i) || (cells[selected] !== 0 && cells[selected] === v))}
    <button
      class="cell"
      class:given={given[i] !== 0}
      class:sel
      class:related
      class:conflict={conflicts.has(i)}
      class:wrong={wrong.has(i)}
      class:bt={Math.floor(i / 9) % 3 === 0 && i >= 9}
      class:bl={i % 3 === 0 && i % 9 !== 0}
      class:brow={Math.floor(i / 9) % 3 === 0 && Math.floor(i / 9) !== 0}
      onclick={() => onselect(i)}
      aria-label={`row ${Math.floor(i / 9) + 1} column ${(i % 9) + 1} ${v || "empty"}`}
    >{#if v}{v}{:else if notes[i]}<span class="notes">{#each [1, 2, 3, 4, 5, 6, 7, 8, 9] as d}<i>{notes[i] & (1 << (d - 1)) ? d : ""}</i>{/each}</span>{/if}</button>
  {/each}
</div>

<style>
  .board { display: grid; grid-template-columns: repeat(9, 1fr); width: min(560px, 90vw); aspect-ratio: 1;
           border: 3px solid var(--line-strong); background: var(--cell); }
  .cell { font: 500 clamp(18px, 4vw, 28px) system-ui, sans-serif; border: 1px solid var(--line); background: var(--cell);
          color: var(--user); padding: 0; cursor: pointer; border-radius: 0; }
  .notes { display: grid; grid-template-columns: repeat(3, 1fr); width: 100%; height: 100%; }
  .notes i { font: 500 clamp(8px, 1.5vw, 11px) system-ui, sans-serif; font-style: normal; color: var(--fg); opacity: 0.7;
             display: flex; align-items: center; justify-content: center; }
  .cell.given { color: var(--given); font-weight: 700; }
  .cell.related { background: var(--related); }
  .cell.sel { background: var(--sel); }
  .cell.conflict { color: var(--bad); }
  .cell.wrong { background: var(--bad-bg); }
  .cell.bl { border-left: 2px solid var(--line-strong); }
  .cell.brow { border-top: 2px solid var(--line-strong); }
</style>
