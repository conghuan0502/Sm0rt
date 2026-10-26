<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Menu } from "@tauri-apps/api/menu";
  import { getMascot, setMascotVisible, showMain, type Mascot, type Stage } from "../lib/api";

  let mascot = $state<Mascot | null>(null);
  let bubble = $state("");
  let cheering = $state(false);
  const stage = $derived<Stage>(mascot?.stage ?? "content");

  // Placeholder vector look per stage; swap for real art later.
  const look: Record<Stage, { body: string; ring: string; mouth: string; label: string }> = {
    thriving: { body: "#ffb347", ring: "#f08a24", mouth: "M40 78 Q60 98 80 78", label: "Thriving" },
    content: { body: "#f4a261", ring: "#e76f51", mouth: "M42 80 Q60 92 78 80", label: "Content" },
    bored: { body: "#d9b38c", ring: "#a8845f", mouth: "M44 84 L76 84", label: "Bored" },
    glazed: { body: "#b9c4a0", ring: "#7f8c6a", mouth: "M44 86 Q60 78 76 86", label: "Glazed" },
    rotted: { body: "#8fb08a", ring: "#5b7a57", mouth: "M42 88 Q60 74 78 88", label: "Brain-rotted" },
  };
  const l = $derived(look[stage]);

  const lines: Record<Stage, string[]> = {
    thriving: ["Brain: fully charged!", "Hit me with a puzzle!", "Feeling sharp today."],
    content: ["A puzzle would be nice.", "Doing okay. Sudoku?", "Let's keep the streak going."],
    bored: ["So... bored...", "Scrolling again? Puzzle?", "My neurons are napping."],
    glazed: ["Duh... what's a number?", "Everything is a blur.", "Please. Just one puzzle."],
    rotted: ["skibidi... brain... help", "I forgot how crosswords work.", "Feed me a Sudoku."],
  };
  const cheers = ["Yum! Brain food.", "Nice one!", "Neurons online!", "More of that, please."];
  const grumbles = ["I feel my brain rotting...", "Is anyone there?", "Getting foggy in here."];
  const pick = (a: string[]) => a[Math.floor(Math.random() * a.length)];

  let bubbleTimer: ReturnType<typeof setTimeout> | undefined;
  function say(text: string, ms = 3500) {
    bubble = text;
    clearTimeout(bubbleTimer);
    bubbleTimer = setTimeout(() => (bubble = ""), ms);
  }

  function cheer() {
    cheering = true;
    setTimeout(() => (cheering = false), 1300);
  }

  const NAG_EVERY_MS = 30 * 60 * 1000;
  let lastNag = Date.now(); // not right at launch
  const order: Stage[] = ["rotted", "glazed", "bored", "content", "thriving"];

  async function refresh() {
    try {
      const prev = mascot;
      const next = await getMascot();
      mascot = next;
      if (prev && next.health > prev.health + 1) {
        cheer();
        say(pick(cheers));
      } else if (prev && order.indexOf(next.stage) < order.indexOf(prev.stage)) {
        say(pick(grumbles), 4500);
      } else if (next.stage !== "thriving" && next.stage !== "content" && Date.now() - lastNag > NAG_EVERY_MS) {
        lastNag = Date.now();
        say(pick(lines[next.stage]), 5000);
      }
    } catch (e) {
      console.error(e);
    }
  }

  // --- pointer: drag the window, click to talk, double-click to open, right-click for a menu ---
  let down: { x: number; y: number } | null = null;
  let dragging = false;
  let lastClick = 0;

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    down = { x: e.screenX, y: e.screenY };
    dragging = false;
  }
  function onpointermove(e: PointerEvent) {
    if (!down || dragging) return;
    if (Math.hypot(e.screenX - down.x, e.screenY - down.y) > 4) {
      dragging = true;
      down = null;
      getCurrentWindow().startDragging().catch(() => {});
    }
  }
  function onpointerup() {
    if (down && !dragging) {
      const now = Date.now();
      if (now - lastClick < 350) {
        lastClick = 0;
        showMain();
      } else {
        lastClick = now;
        cheer();
        say(pick(lines[stage]));
      }
    }
    down = null;
  }

  async function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        { id: "sudoku", text: "Play Sudoku", action: () => showMain("sudoku") },
        { id: "crossword", text: "Play Crossword", action: () => showMain("crossword") },
        { id: "open", text: "Open Sm0rt", action: () => showMain() },
        { item: "Separator" },
        { id: "hide", text: "Hide mascot (restore from tray)", action: () => setMascotVisible(false) },
      ],
    });
    await menu.popup();
  }

  onMount(() => {
    refresh();
    const timer = setInterval(refresh, 30_000);
    const unlisten = listen("mascot-updated", refresh);
    return () => {
      clearInterval(timer);
      clearTimeout(bubbleTimer);
      unlisten.then((f) => f());
    };
  });
</script>

<!-- 240x240 window; the 120px body sits at (60,90): Rust hit-test circle is centre (120,150), r 60. -->
{#if bubble}<div class="bubble" role="status">{bubble}</div>{/if}
<div
  class={`sprite ${stage}`}
  class:cheer={cheering}
  role="button"
  tabindex="-1"
  title={`${l.label} — click to chat, double-click to open, right-click for more`}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  {oncontextmenu}
>
  <svg viewBox="0 0 120 120" width="120" height="120">
    <circle cx="60" cy="60" r="56" fill={l.body} stroke={l.ring} stroke-width="4" />
    {#if stage === "thriving"}
      <g class="eyes"><circle cx="44" cy="52" r="6" fill="#222" /><circle cx="76" cy="52" r="6" fill="#222" />
      <circle cx="46" cy="50" r="2" fill="#fff" /><circle cx="78" cy="50" r="2" fill="#fff" /></g>
    {:else if stage === "content"}
      <g class="eyes"><circle cx="44" cy="52" r="5" fill="#222" /><circle cx="76" cy="52" r="5" fill="#222" /></g>
    {:else if stage === "bored"}
      <rect x="37" y="50" width="14" height="4" rx="2" fill="#222" /><rect x="69" y="50" width="14" height="4" rx="2" fill="#222" />
    {:else if stage === "glazed"}
      <ellipse cx="44" cy="52" rx="8" ry="5" fill="#fff" /><circle cx="44" cy="55" r="3" fill="#222" />
      <ellipse cx="76" cy="52" rx="8" ry="5" fill="#fff" /><circle cx="76" cy="55" r="3" fill="#222" />
    {:else}
      <g fill="none" stroke="#222" stroke-width="2.5">
        <path d="M44 52 m-2 0 a2 2 0 1 1 4 0 a4 4 0 1 1 -8 0 a6 6 0 1 1 12 0" />
        <path d="M76 52 m-2 0 a2 2 0 1 1 4 0 a4 4 0 1 1 -8 0 a6 6 0 1 1 12 0" />
      </g>
      <path class="drool" d="M70 90 q4 8 0 14" stroke="#9ad" stroke-width="3" fill="none" />
    {/if}
    <path d={l.mouth} stroke="#222" stroke-width="3" fill="none" stroke-linecap="round" />
  </svg>
  {#if mascot}<div class="badge">🔥 {mascot.streak}</div>{/if}
</div>

<style>
  :global(html, body) { margin: 0; background: transparent; overflow: hidden; user-select: none; }
  .sprite { position: absolute; left: 60px; top: 90px; width: 120px; height: 120px; cursor: grab; transform-origin: 50% 100%; }
  .sprite:active { cursor: grabbing; }
  svg { pointer-events: none; display: block; }
  .badge { position: absolute; left: 92px; top: 100px; font: 600 13px system-ui, sans-serif; color: #fff;
           background: #0008; border-radius: 10px; padding: 1px 7px; pointer-events: none; }
  .bubble { position: absolute; left: 10px; right: 10px; top: 8px; max-height: 72px; overflow: hidden; box-sizing: border-box;
            padding: 6px 10px; border-radius: 12px; background: #fffef8; color: #2b2622; border: 2px solid #2b2622;
            font: 600 13px/1.25 system-ui, sans-serif; text-align: center; pointer-events: none; }
  .bubble::after { content: ""; position: absolute; left: 50%; bottom: -9px; margin-left: -7px; width: 12px; height: 12px;
                   background: #fffef8; border-right: 2px solid #2b2622; border-bottom: 2px solid #2b2622; transform: rotate(45deg); }

  @media (prefers-reduced-motion: no-preference) {
    .thriving { animation: bounce 1.6s ease-in-out infinite; }
    .content { animation: sway 4s ease-in-out infinite; }
    .bored { animation: slump 5s ease-in-out infinite; }
    .glazed { animation: wobble 3s ease-in-out infinite; }
    .rotted { animation: twitch 2.4s steps(1) infinite; }
    .cheer { animation: jump 0.65s ease-out 2 !important; }
    .eyes { animation: blink 5s infinite; transform-origin: 60px 52px; }
  }
  @keyframes bounce { 0%, 100% { transform: translateY(0) scale(1, 1); } 50% { transform: translateY(-5px) scale(0.98, 1.03); } }
  @keyframes sway { 0%, 100% { transform: rotate(-2deg); } 50% { transform: rotate(2deg); } }
  @keyframes slump { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.03, 0.95); } }
  @keyframes wobble { 0%, 100% { transform: rotate(-4deg) translateX(-2px); } 50% { transform: rotate(4deg) translateX(2px); } }
  @keyframes twitch { 0% { transform: translate(0, 0); } 70% { transform: translate(2px, -1px) rotate(1deg); } 80% { transform: translate(-2px, 1px) rotate(-1.5deg); } 90% { transform: translate(0, 0); } }
  @keyframes jump { 0% { transform: translateY(0) scale(1, 1); } 40% { transform: translateY(-22px) scale(0.95, 1.08); } 100% { transform: translateY(0) scale(1, 1); } }
  @keyframes blink { 0%, 94%, 100% { transform: scaleY(1); } 97% { transform: scaleY(0.1); } }
</style>
