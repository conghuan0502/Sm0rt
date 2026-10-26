// Builds data/clues.tsv from WordNet 3.1 (license: https://wordnet.princeton.edu/license-and-commercial-use).
// Output columns: WORD<TAB>FREQ<TAB>CLUE   (FREQ = WordNet tagged-sense count, a popularity proxy)
// Attribution for WordNet must ship with the app.
//
// Clue quality heuristics (v2): look at the first few senses, trim glosses to crossword length,
// drop leading articles, reject clues that leak the answer or a close inflection of it, and prefer
// the first sense that reads like a clue. Hand/LLM-written clues in data/clue_overrides.tsv
// (WORD<TAB>CLUE) always win.
import fs from "node:fs";
import path from "node:path";

const dict = path.resolve("node_modules/wordnet-db/dict");
const out = path.resolve("../../data/clues.tsv");
const overridesFile = path.resolve("../../data/clue_overrides.tsv");
const MIN_LEN = 3;
const MAX_LEN = 9;
const SENSES = 3;
const GOOD_MIN = 10;
const GOOD_MAX = 52;

function loadGlosses(file) {
  const m = new Map();
  for (const line of fs.readFileSync(path.join(dict, file), "utf8").split("\n")) {
    if (line.startsWith("  ") || !line) continue;
    const bar = line.indexOf("|");
    if (bar < 0) continue;
    m.set(line.slice(0, 8), line.slice(bar + 1).trim());
  }
  return m;
}

/** Shared prefix that also catches inflections (running/RUN, stratified/STRATIFY). */
function leaks(clue, word) {
  const stem = word.toLowerCase().slice(0, Math.max(3, word.length - 2));
  return new RegExp(`\\b${stem}`, "i").test(clue);
}

function cleanGloss(g, word) {
  let c = g.split(";")[0];
  c = c.replace(/\([^)]*\)/g, "").replace(/\s+/g, " ").trim();
  c = c.replace(/^(a|an|the)\s+/i, "");
  // Long glosses: cut at the first natural break after a few words.
  if (c.length > GOOD_MAX) {
    const cut = c.search(/,| especially| usually| having| that | which | used /);
    if (cut >= 12) c = c.slice(0, cut).trim();
  }
  c = c.replace(/[,:\s]+$/, "");
  if (c.length < 6 || c.length > 70) return null;
  if (leaks(c, word)) return null;
  return c.charAt(0).toUpperCase() + c.slice(1);
}

const isGood = (c) => c.length >= GOOD_MIN && c.length <= GOOD_MAX;

const best = new Map(); // WORD -> {freq, clue, good}
for (const pos of ["noun", "verb", "adj"]) {
  const glosses = loadGlosses(`data.${pos}`);
  for (const line of fs.readFileSync(path.join(dict, `index.${pos}`), "utf8").split("\n")) {
    if (line.startsWith("  ") || !line) continue;
    const f = line.split(" ");
    const lemma = f[0];
    if (!/^[a-z]+$/.test(lemma) || lemma.length < MIN_LEN || lemma.length > MAX_LEN) continue;
    const synsetCnt = parseInt(f[2], 10);
    const ptrCnt = parseInt(f[3], 10);
    const tagsense = parseInt(f[5 + ptrCnt], 10);
    const offsets = f.slice(6 + ptrCnt, 6 + ptrCnt + Math.min(synsetCnt, SENSES));
    const cands = offsets.map((o) => cleanGloss(glosses.get(o) ?? "", lemma)).filter(Boolean);
    if (!cands.length) continue;
    const clue = cands.find(isGood) ?? cands[0];
    const word = lemma.toUpperCase();
    const prev = best.get(word);
    if (!prev || tagsense > prev.freq) best.set(word, { freq: tagsense, clue, good: isGood(clue) });
  }
}

let overridden = 0;
if (fs.existsSync(overridesFile)) {
  for (const line of fs.readFileSync(overridesFile, "utf8").split("\n")) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [w, clue] = line.split("\t");
    const word = w?.trim().toUpperCase();
    if (!word || !clue || !best.has(word) || leaks(clue, word)) continue;
    best.get(word).clue = clue.trim();
    best.get(word).good = true;
    overridden++;
  }
}

fs.mkdirSync(path.dirname(out), { recursive: true });
const rows = [...best.entries()]
  .sort((a, b) => b[1].freq - a[1].freq || a[0].localeCompare(b[0]))
  .map(([w, v]) => `${w}\t${v.freq}\t${v.clue}`);
fs.writeFileSync(out, rows.join("\n") + "\n");
const common = [...best.values()].filter((v) => v.freq > 0);
console.log(`wrote ${rows.length} entries (${overridden} overrides); of ${common.length} with freq>0, ` +
  `${common.filter((v) => v.good).length} have a clue of ideal length`);
