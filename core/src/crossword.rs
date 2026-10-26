//! Crossword generation: template grids filled by MRV backtracking over a
//! letter-position indexed word list, with clues from the bundled table.

use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashSet;

pub struct Entry {
    pub word: String,
    pub freq: u32,
    pub clue: String,
}

pub struct WordList {
    pub entries: Vec<Entry>,
    /// by_len[len] = ids of entries with that length.
    by_len: Vec<Vec<u32>>,
    /// idx[len][pos][letter] = ids (ascending) with `letter` at `pos`.
    idx: Vec<Vec<[Vec<u32>; 26]>>,
}

const MAX_LEN: usize = 15;

impl WordList {
    /// Parses `WORD<TAB>FREQ<TAB>CLUE` lines, keeping entries with freq >= min_freq.
    pub fn from_tsv(tsv: &str, min_freq: u32) -> WordList {
        let mut entries = Vec::new();
        for line in tsv.lines() {
            let mut f = line.splitn(3, '\t');
            let (Some(w), Some(fr), Some(c)) = (f.next(), f.next(), f.next()) else { continue };
            let Ok(freq) = fr.parse::<u32>() else { continue };
            if freq < min_freq || w.len() > MAX_LEN || !w.bytes().all(|b| b.is_ascii_uppercase()) {
                continue;
            }
            entries.push(Entry { word: w.to_string(), freq, clue: c.to_string() });
        }
        let mut by_len = vec![Vec::new(); MAX_LEN + 1];
        let mut idx: Vec<Vec<[Vec<u32>; 26]>> = (0..=MAX_LEN)
            .map(|l| (0..l).map(|_| std::array::from_fn(|_| Vec::new())).collect())
            .collect();
        for (id, e) in entries.iter().enumerate() {
            let l = e.word.len();
            by_len[l].push(id as u32);
            for (p, b) in e.word.bytes().enumerate() {
                idx[l][p][(b - b'A') as usize].push(id as u32);
            }
        }
        WordList { entries, by_len, idx }
    }

    /// Ids of words of length `pattern.len()` matching `pattern` (None = wildcard).
    fn matching(&self, pattern: &[Option<u8>]) -> Vec<u32> {
        let l = pattern.len();
        let fixed: Vec<(usize, u8)> =
            pattern.iter().enumerate().filter_map(|(p, c)| c.map(|c| (p, c))).collect();
        let Some(&(p0, c0)) = fixed.iter().min_by_key(|(p, c)| self.idx[l][*p][(*c - b'A') as usize].len())
        else {
            return self.by_len[l].clone();
        };
        self.idx[l][p0][(c0 - b'A') as usize]
            .iter()
            .copied()
            .filter(|&id| {
                let w = self.entries[id as usize].word.as_bytes();
                fixed.iter().all(|&(p, c)| w[p] == c)
            })
            .collect()
    }

    pub fn contains(&self, word: &str) -> bool {
        let l = word.len();
        l <= MAX_LEN && self.by_len[l].iter().any(|&id| self.entries[id as usize].word == word)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Across,
    Down,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub dir: Dir,
    pub cells: Vec<usize>,
}

pub struct Template {
    pub size: usize,
    pub blocks: Vec<bool>,
}

impl Template {
    /// `#` = block, anything else = open cell. Rows separated by '/'.
    pub fn parse(s: &str) -> Template {
        let rows: Vec<&str> = s.split('/').collect();
        let size = rows.len();
        assert!(rows.iter().all(|r| r.len() == size));
        Template { size, blocks: rows.iter().flat_map(|r| r.bytes().map(|b| b == b'#')).collect() }
    }

    pub fn slots(&self) -> Vec<Slot> {
        let n = self.size;
        let mut out = Vec::new();
        for dir in [Dir::Across, Dir::Down] {
            for a in 0..n {
                let mut run: Vec<usize> = Vec::new();
                for b in 0..=n {
                    let cell = if b < n {
                        let i = if dir == Dir::Across { a * n + b } else { b * n + a };
                        (!self.blocks[i]).then_some(i)
                    } else {
                        None
                    };
                    match cell {
                        Some(i) => run.push(i),
                        None => {
                            if run.len() >= 3 {
                                out.push(Slot { dir, cells: std::mem::take(&mut run) });
                            }
                            run.clear();
                        }
                    }
                }
            }
        }
        out
    }
}

pub const MINI_TEMPLATES: [&str; 3] = [
    "...../.#.#./...../.#.#./.....",
    "...##/....#/...../#..../##...",
    "....#/....#/...../#..../#....",
];

/// Random 180°-symmetric block layout. Every open cell must lie in a slot of
/// length >= 3, no run may have length 2, and open cells must be connected.
pub fn random_template<R: Rng>(size: usize, rng: &mut R) -> Template {
    let n = size * size;
    let target = n * 22 / 100;
    loop {
        let mut blocks = vec![false; n];
        let mut count = 0;
        let mut tries = 0;
        while count < target && tries < 200 {
            tries += 1;
            let i = rng.gen_range(0..n);
            let j = n - 1 - i;
            if blocks[i] {
                continue;
            }
            blocks[i] = true;
            blocks[j] = true;
            let t = Template { size, blocks: blocks.clone() };
            if t.is_valid_partial() {
                count += if i == j { 1 } else { 2 };
            } else {
                blocks[i] = false;
                blocks[j] = false;
            }
        }
        let t = Template { size, blocks };
        if count >= target * 3 / 4 && t.is_valid() {
            return t;
        }
    }
}

impl Template {
    /// No run of exactly 2 (cheap check usable while building).
    fn is_valid_partial(&self) -> bool {
        let n = self.size;
        for a in 0..n {
            for horiz in [true, false] {
                let mut run = 0;
                for b in 0..=n {
                    let open = b < n && !self.blocks[if horiz { a * n + b } else { b * n + a }];
                    if open {
                        run += 1;
                    } else {
                        if run == 2 {
                            return false;
                        }
                        run = 0;
                    }
                }
            }
        }
        true
    }

    /// Partial rules plus: every open cell is in a slot, and open cells are connected.
    pub fn is_valid(&self) -> bool {
        if !self.is_valid_partial() {
            return false;
        }
        let n = self.size;
        let mut covered = vec![false; n * n];
        for s in self.slots() {
            for c in s.cells {
                covered[c] = true;
            }
        }
        let open: Vec<usize> = (0..n * n).filter(|&i| !self.blocks[i]).collect();
        if open.is_empty() || open.iter().any(|&i| !covered[i]) {
            return false;
        }
        let mut seen = vec![false; n * n];
        let mut stack = vec![open[0]];
        seen[open[0]] = true;
        while let Some(i) = stack.pop() {
            let (r, c) = (i / n, i % n);
            let mut nb = Vec::new();
            if r > 0 { nb.push(i - n); }
            if r + 1 < n { nb.push(i + n); }
            if c > 0 { nb.push(i - 1); }
            if c + 1 < n { nb.push(i + 1); }
            for j in nb {
                if !self.blocks[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        open.iter().all(|&i| seen[i])
    }
}

/// Crossword difficulty: grid size and how common the answers must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Easy,
    Medium,
    Hard,
}

impl Tier {
    /// Minimum WordNet frequency of answers.
    pub fn min_freq(self) -> u32 {
        match self {
            Tier::Easy => 2,
            Tier::Medium | Tier::Hard => 1,
        }
    }
    pub fn size(self) -> usize {
        match self {
            Tier::Easy | Tier::Medium => 5,
            Tier::Hard => 9,
        }
    }
}

pub struct FillOptions {
    pub max_nodes: u64,
}

pub struct Crossword {
    pub size: usize,
    /// Row-major letters; b'#' for blocks.
    pub cells: Vec<u8>,
    pub slots: Vec<Slot>,
    pub entry_ids: Vec<u32>,
}

struct Search<'a> {
    wl: &'a WordList,
    slots: &'a [Slot],
    grid: Vec<u8>, // 0 = empty
    chosen: Vec<Option<u32>>,
    used: HashSet<u32>,
    nodes: u64,
    max_nodes: u64,
}

impl Search<'_> {
    fn pattern(&self, s: &Slot) -> Vec<Option<u8>> {
        s.cells.iter().map(|&i| (self.grid[i] != 0).then_some(self.grid[i])).collect()
    }

    fn run<R: Rng>(&mut self, rng: &mut R) -> bool {
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            return false;
        }
        // MRV: unfilled slot with fewest candidates.
        let mut best: Option<(usize, Vec<u32>)> = None;
        for (k, s) in self.slots.iter().enumerate() {
            if self.chosen[k].is_some() {
                continue;
            }
            let mut c = self.wl.matching(&self.pattern(s));
            c.retain(|id| !self.used.contains(id));
            if c.is_empty() {
                return false;
            }
            if best.as_ref().map_or(true, |(_, b)| c.len() < b.len()) {
                best = Some((k, c));
            }
        }
        let Some((k, mut cands)) = best else { return true };
        cands.shuffle(rng);
        for id in cands.into_iter().take(40) {
            let word = self.wl.entries[id as usize].word.as_bytes();
            let cells = &self.slots[k].cells;
            let saved: Vec<u8> = cells.iter().map(|&i| self.grid[i]).collect();
            for (p, &i) in cells.iter().enumerate() {
                self.grid[i] = word[p];
            }
            self.chosen[k] = Some(id);
            self.used.insert(id);
            if self.run(rng) {
                return true;
            }
            self.used.remove(&id);
            self.chosen[k] = None;
            for (p, &i) in self.slots[k].cells.iter().enumerate() {
                self.grid[i] = saved[p];
            }
            if self.nodes > self.max_nodes {
                return false;
            }
        }
        false
    }
}

/// Fill a template; restarts internally are left to the caller (see `generate`).
pub fn fill<R: Rng>(wl: &WordList, t: &Template, rng: &mut R, opts: &FillOptions) -> Option<Crossword> {
    let slots = t.slots();
    let mut s = Search {
        wl,
        slots: &slots,
        grid: vec![0; t.size * t.size],
        chosen: vec![None; slots.len()],
        used: HashSet::new(),
        nodes: 0,
        max_nodes: opts.max_nodes,
    };
    if !s.run(rng) {
        return None;
    }
    let cells = (0..t.size * t.size).map(|i| if t.blocks[i] { b'#' } else { s.grid[i] }).collect();
    let entry_ids = s.chosen.iter().map(|c| c.unwrap()).collect();
    Some(Crossword { size: t.size, cells, slots, entry_ids })
}

/// Pick random templates and restart until a fill succeeds.
pub fn generate<R: Rng>(wl: &WordList, templates: &[&str], rng: &mut R, opts: &FillOptions, attempts: usize) -> Option<Crossword> {
    for _ in 0..attempts {
        let t = Template::parse(templates.choose(rng)?);
        if let Some(c) = fill(wl, &t, rng, opts) {
            return Some(c);
        }
    }
    None
}

/// Generate for a tier. `wl` must have been built with `tier.min_freq()`.
pub fn generate_tier<R: Rng>(wl: &WordList, tier: Tier, rng: &mut R, attempts: usize) -> Option<Crossword> {
    let opts = FillOptions { max_nodes: if tier == Tier::Hard { 500 } else { 20_000 } };
    for _ in 0..attempts {
        let t = if tier.size() == 5 {
            Template::parse(MINI_TEMPLATES.choose(rng)?)
        } else {
            random_template(tier.size(), rng)
        };
        if let Some(c) = fill(wl, &t, rng, &opts) {
            return Some(c);
        }
    }
    None
}

impl Crossword {
    /// Independent re-check: every run of >= 3 letters is a distinct list word
    /// and every slot's clue does not contain its answer.
    pub fn validate(&self, wl: &WordList) -> Result<(), String> {
        let n = self.size;
        let mut seen = HashSet::new();
        for dir in [Dir::Across, Dir::Down] {
            for a in 0..n {
                let mut run = String::new();
                for b in 0..=n {
                    let ch = if b < n {
                        let i = if dir == Dir::Across { a * n + b } else { b * n + a };
                        self.cells[i]
                    } else {
                        b'#'
                    };
                    if ch == b'#' || ch == 0 {
                        if run.len() >= 3 {
                            if !wl.contains(&run) {
                                return Err(format!("{run} not in word list"));
                            }
                            if !seen.insert(run.clone()) {
                                return Err(format!("{run} duplicated"));
                            }
                        } else if !run.is_empty() && run.len() != 1 {
                            return Err(format!("{run} too short"));
                        }
                        run.clear();
                    } else {
                        run.push(ch as char);
                    }
                }
            }
        }
        for (k, s) in self.slots.iter().enumerate() {
            let e = &wl.entries[self.entry_ids[k] as usize];
            let word: String = s.cells.iter().map(|&i| self.cells[i] as char).collect();
            if word != e.word {
                return Err(format!("slot {k} mismatch {word} vs {}", e.word));
            }
            // Word-level check (matches tools/pipeline): "sovereign" may contain REIGN, but "reigning" may not.
            let stem = &e.word[..e.word.len().saturating_sub(2).max(3).min(e.word.len())];
            let leaks = e
                .clue
                .to_uppercase()
                .split(|c: char| !c.is_ascii_alphabetic())
                .any(|t| t.starts_with(stem));
            if leaks {
                return Err(format!("clue leaks {}", e.word));
            }
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        self.cells
            .chunks(self.size)
            .map(|r| r.iter().map(|&b| b as char).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::time::Instant;

    fn list(min_freq: u32) -> WordList {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/clues.tsv");
        WordList::from_tsv(&std::fs::read_to_string(p).expect("run tools/pipeline/build_clues.mjs"), min_freq)
    }

    #[test]
    fn template_slots_are_extracted() {
        let t = Template::parse(MINI_TEMPLATES[0]);
        assert_eq!(t.slots().len(), 6);
    }

    #[test]
    fn mini_fills_are_valid() {
        let wl = list(1);
        let mut rng = StdRng::seed_from_u64(5);
        let opts = FillOptions { max_nodes: 20_000 };
        for _ in 0..30 {
            let c = generate(&wl, &MINI_TEMPLATES, &mut rng, &opts, 50).expect("fill");
            c.validate(&wl).unwrap();
        }
    }

    #[test]
    fn random_templates_are_valid_and_symmetric() {
        let mut rng = StdRng::seed_from_u64(2);
        for size in [5, 7, 9] {
            for _ in 0..50 {
                let t = random_template(size, &mut rng);
                assert!(t.is_valid());
                let n = size * size;
                assert!((0..n).all(|i| t.blocks[i] == t.blocks[n - 1 - i]));
            }
        }
    }

    #[test]
    fn tiers_fill_and_validate() {
        let mut rng = StdRng::seed_from_u64(21);
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            let wl = list(tier.min_freq());
            let n = 8;
            let mut times = Vec::new();
            let mut fails = 0;
            for i in 0..n {
                let t = Instant::now();
                match generate_tier(&wl, tier, &mut rng, 400) {
                    Some(c) => {
                        c.validate(&wl).unwrap();
                        if i == 0 {
                            println!("{:?}\n{}", tier, c.render());
                        }
                    }
                    None => fails += 1,
                }
                times.push(t.elapsed().as_millis());
            }
            times.sort();
            println!("{:?}: words={} fails={}/{} p50={}ms max={}ms", tier, wl.entries.len(), fails, n, times[n / 2], times[n - 1]);
            assert_eq!(fails, 0, "{:?}", tier);
        }
    }

    /// Tail-latency check for the 9x9 tier (`cargo test --release hard_tail -- --nocapture --ignored`).
    #[test]
    #[ignore]
    fn hard_tail_latency() {
        let wl = list(Tier::Hard.min_freq());
        let mut rng = StdRng::seed_from_u64(77);
        let n = 60;
        let mut times: Vec<u128> = (0..n)
            .map(|_| {
                let t = Instant::now();
                generate_tier(&wl, Tier::Hard, &mut rng, 2000).expect("fill");
                t.elapsed().as_millis()
            })
            .collect();
        times.sort();
        println!("hard: p50={}ms p90={}ms p99={}ms max={}ms", times[n / 2], times[n * 9 / 10], times[n * 99 / 100], times[n - 1]);
    }

    #[test]
    fn bench_mini() {
        let wl = list(1);
        println!("word list: {} entries", wl.entries.len());
        let mut rng = StdRng::seed_from_u64(9);
        let opts = FillOptions { max_nodes: 20_000 };
        let n = 50;
        let mut times = Vec::new();
        let mut fails = 0;
        for i in 0..n {
            let t = Instant::now();
            match generate(&wl, &MINI_TEMPLATES, &mut rng, &opts, 50) {
                Some(c) => {
                    if i == 0 {
                        println!("{}", c.render());
                    }
                }
                None => fails += 1,
            }
            times.push(t.elapsed().as_millis());
        }
        times.sort();
        println!("mini: fails={fails} p50={}ms p95={}ms max={}ms", times[n / 2], times[n * 95 / 100], times[n - 1]);
        assert_eq!(fails, 0);
        assert!(times[n * 95 / 100] < 1000);
    }
}
