<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Sudoku from "./Sudoku.svelte";
  import Crossword from "./Crossword.svelte";
  import { getMascot, type Mascot, type Tab } from "../lib/api";

  let tab = $state<Tab>("sudoku");
  let mascot = $state<Mascot | null>(null);

  const stageLabel: Record<string, string> = {
    thriving: "Thriving", content: "Content", bored: "Bored", glazed: "Glazed", rotted: "Brain-rotted",
  };

  async function refreshMascot() {
    try { mascot = await getMascot(); } catch (e) { console.error(e); }
  }

  onMount(() => {
    refreshMascot();
    const unlisten = listen("mascot-updated", refreshMascot);
    const unlistenTab = listen<Tab>("open-tab", (e) => (tab = e.payload));
    return () => { unlisten.then((f) => f()); unlistenTab.then((f) => f()); };
  });
</script>

<main>
  <header>
    <h1>Sm0rt</h1>
    {#if mascot}
      <div class="status">
        <span>{stageLabel[mascot.stage]}</span>
        <div class="bar" title={`Health ${Math.round(mascot.health)}`}><div style={`width:${mascot.health}%`}></div></div>
        <span>🔥 {mascot.streak}-day streak</span>
        {#if mascot.freezes > 0}<span>❄️ {mascot.freezes}</span>{/if}
      </div>
    {/if}
  </header>

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "sudoku"} class:on={tab === "sudoku"} onclick={() => (tab = "sudoku")}>Sudoku</button>
    <button role="tab" aria-selected={tab === "crossword"} class:on={tab === "crossword"} onclick={() => (tab = "crossword")}>Crossword</button>
  </div>

  <!-- Both stay mounted so an unfinished puzzle survives switching tabs. -->
  <div class="pane" hidden={tab !== "sudoku"}><Sudoku active={tab === "sudoku"} /></div>
  <div class="pane" hidden={tab !== "crossword"}><Crossword active={tab === "crossword"} /></div>
</main>

<style>
  :global(:root) {
    --bg: #faf7f2; --fg: #2b2622; --cell: #fff; --line: #d9d2c7; --line-strong: #2b2622;
    --given: #2b2622; --user: #2a6fdb; --sel: #ffe29a; --related: #f1ebe0; --bad: #d1322b; --bad-bg: #fbd5d2;
    --accent: #e76f51;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #1c1a18; --fg: #efe9e1; --cell: #26231f; --line: #433e37; --line-strong: #cfc6b8;
      --given: #efe9e1; --user: #7fb0ff; --sel: #6b5a21; --related: #302c26; --bad: #ff7a72; --bad-bg: #5a2623;
    }
  }
  :global(body) { margin: 0; background: var(--bg); color: var(--fg); font-family: system-ui, sans-serif; }
  :global(.controls), :global(.pad) { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  :global(button), :global(select) { font: inherit; padding: 6px 14px; border-radius: 8px; border: 1px solid var(--line); background: var(--cell); color: var(--fg); cursor: pointer; }
  :global(button:disabled) { opacity: 0.5; cursor: default; }
  :global(.pad button) { min-width: 44px; font-size: 18px; }
  :global(.win) { color: #2e9e5b; font-weight: 600; margin: 0; }
  :global(.err) { color: var(--bad); margin: 0; }
  main { display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 16px; }
  header { display: flex; align-items: center; gap: 24px; width: min(820px, 94vw); justify-content: space-between; }
  h1 { margin: 0; font-size: 24px; }
  .status { display: flex; align-items: center; gap: 10px; font-size: 14px; }
  .bar { width: 90px; height: 8px; border-radius: 4px; background: var(--line); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.4s; }
  .tabs { display: flex; gap: 6px; }
  .tabs button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .pane { width: 100%; display: flex; justify-content: center; }
  .pane[hidden] { display: none; }
</style>
