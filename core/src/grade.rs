//! Human-technique solver used to grade Sudoku difficulty by the hardest
//! technique required, then to generate puzzles of a requested tier.

use crate::sudoku::{generate, Grid};
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Technique {
    NakedSingle,
    HiddenSingle,
    LockedCandidates,
    NakedPair,
    HiddenPair,
    XWing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    /// Needs techniques beyond the solver's repertoire (or guessing).
    Expert,
}

#[derive(Debug, Clone)]
pub struct Grading {
    pub difficulty: Difficulty,
    pub hardest: Option<Technique>,
    /// Number of applications per technique, indexed by `Technique as usize`.
    pub steps: [u32; 6],
}

fn units() -> Vec<[usize; 9]> {
    let mut u = Vec::with_capacity(27);
    for r in 0..9 {
        u.push(std::array::from_fn(|c| r * 9 + c));
    }
    for c in 0..9 {
        u.push(std::array::from_fn(|r| r * 9 + c));
    }
    for b in 0..9 {
        let (br, bc) = (b / 3 * 3, b % 3 * 3);
        u.push(std::array::from_fn(|k| (br + k / 3) * 9 + bc + k % 3));
    }
    u
}

struct Board {
    cand: [u16; 81],
    units: Vec<[usize; 9]>,
}

impl Board {
    fn new(grid: &Grid) -> Option<Board> {
        let mut b = Board { cand: [0x1FF; 81], units: units() };
        for (i, &v) in grid.iter().enumerate() {
            if v != 0 && !b.place(i, v) {
                return None;
            }
        }
        Some(b)
    }

    fn solved(&self) -> bool {
        self.cand.iter().all(|c| c.count_ones() == 1)
    }

    fn peers(i: usize) -> impl Iterator<Item = usize> {
        let (r, c) = (i / 9, i % 9);
        let (br, bc) = (r / 3 * 3, c / 3 * 3);
        (0..81).filter(move |&j| {
            j != i && (j / 9 == r || j % 9 == c || (j / 9 / 3 * 3 == br && j % 9 / 3 * 3 == bc))
        })
    }

    /// Fix cell to `v` and remove it from peers. False on contradiction.
    fn place(&mut self, i: usize, v: u8) -> bool {
        let bit = 1u16 << (v - 1);
        if self.cand[i] & bit == 0 {
            return false;
        }
        self.cand[i] = bit;
        for j in Self::peers(i) {
            if self.cand[j] & bit != 0 {
                self.cand[j] &= !bit;
                if self.cand[j] == 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Remove `mask` from cell `i`; returns true if something changed.
    fn elim(&mut self, i: usize, mask: u16) -> bool {
        if self.cand[i] & mask != 0 {
            self.cand[i] &= !mask;
            true
        } else {
            false
        }
    }

    fn naked_single(&mut self) -> bool {
        for i in 0..81 {
            let c = self.cand[i];
            if c.count_ones() == 1 {
                // Propagate if any peer still holds this digit.
                let v = c.trailing_zeros() as u8 + 1;
                if Self::peers(i).any(|j| self.cand[j] & c != 0) {
                    return self.place(i, v);
                }
            }
        }
        false
    }

    fn hidden_single(&mut self) -> bool {
        for u in self.units.clone() {
            for d in 0..9 {
                let bit = 1u16 << d;
                let spots: Vec<usize> = u.iter().copied().filter(|&i| self.cand[i] & bit != 0).collect();
                if spots.len() == 1 && self.cand[spots[0]].count_ones() > 1 {
                    return self.place(spots[0], d as u8 + 1);
                }
            }
        }
        false
    }

    fn locked_candidates(&mut self) -> bool {
        let units = self.units.clone();
        let mut changed = false;
        // Pointing (box -> line) and claiming (line -> box).
        for (bi, b) in units[18..].iter().enumerate() {
            let _ = bi;
            for (li, l) in units[..18].iter().enumerate() {
                let _ = li;
                let inter: Vec<usize> = b.iter().copied().filter(|i| l.contains(i)).collect();
                if inter.is_empty() {
                    continue;
                }
                for d in 0..9 {
                    let bit = 1u16 << d;
                    let in_inter = inter.iter().any(|&i| self.cand[i] & bit != 0);
                    if !in_inter {
                        continue;
                    }
                    let box_rest = b.iter().any(|i| !inter.contains(i) && self.cand[*i] & bit != 0);
                    let line_rest = l.iter().any(|i| !inter.contains(i) && self.cand[*i] & bit != 0);
                    if !box_rest && line_rest {
                        for &i in l.iter().filter(|i| !inter.contains(i)) {
                            changed |= self.elim(i, bit);
                        }
                    } else if !line_rest && box_rest {
                        for &i in b.iter().filter(|i| !inter.contains(i)) {
                            changed |= self.elim(i, bit);
                        }
                    }
                }
            }
        }
        changed
    }

    fn naked_pair(&mut self) -> bool {
        let mut changed = false;
        for u in self.units.clone() {
            for a in 0..9 {
                if self.cand[u[a]].count_ones() != 2 {
                    continue;
                }
                for b in a + 1..9 {
                    if self.cand[u[b]] == self.cand[u[a]] {
                        let m = self.cand[u[a]];
                        for k in 0..9 {
                            if k != a && k != b {
                                changed |= self.elim(u[k], m);
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    fn hidden_pair(&mut self) -> bool {
        let mut changed = false;
        for u in self.units.clone() {
            let pos: [u16; 9] = std::array::from_fn(|d| {
                u.iter().enumerate().fold(0u16, |m, (k, &i)| if self.cand[i] >> d & 1 == 1 { m | 1 << k } else { m })
            });
            for d1 in 0..9 {
                for d2 in d1 + 1..9 {
                    if pos[d1].count_ones() == 2 && pos[d1] == pos[d2] {
                        let keep = (1u16 << d1) | (1 << d2);
                        for k in 0..9 {
                            if pos[d1] >> k & 1 == 1 {
                                changed |= self.elim(u[k], !keep & 0x1FF);
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    fn x_wing(&mut self) -> bool {
        let mut changed = false;
        for d in 0..9 {
            let bit = 1u16 << d;
            for by_row in [true, false] {
                let idx = |line: usize, k: usize| if by_row { line * 9 + k } else { k * 9 + line };
                let masks: Vec<u16> = (0..9)
                    .map(|l| (0..9).fold(0u16, |m, k| if self.cand[idx(l, k)] & bit != 0 { m | 1 << k } else { m }))
                    .collect();
                for l1 in 0..9 {
                    if masks[l1].count_ones() != 2 {
                        continue;
                    }
                    for l2 in l1 + 1..9 {
                        if masks[l2] != masks[l1] {
                            continue;
                        }
                        for k in (0..9).filter(|k| masks[l1] >> k & 1 == 1) {
                            for l in (0..9).filter(|&l| l != l1 && l != l2) {
                                changed |= self.elim(idx(l, k), bit);
                            }
                        }
                    }
                }
            }
        }
        changed
    }
}

fn has_empty(b: &Board) -> bool {
    b.cand.iter().any(|&c| c == 0)
}

/// Grade a puzzle by solving it with the easiest applicable technique first.
pub fn grade(grid: &Grid) -> Grading {
    let mut steps = [0u32; 6];
    let mut hardest: Option<Technique> = None;
    let Some(mut b) = Board::new(grid) else {
        return Grading { difficulty: Difficulty::Expert, hardest, steps };
    };
    type Step = (Technique, fn(&mut Board) -> bool);
    let ladder: [Step; 6] = [
        (Technique::NakedSingle, Board::naked_single),
        (Technique::HiddenSingle, Board::hidden_single),
        (Technique::LockedCandidates, Board::locked_candidates),
        (Technique::NakedPair, Board::naked_pair),
        (Technique::HiddenPair, Board::hidden_pair),
        (Technique::XWing, Board::x_wing),
    ];
    'outer: while !b.solved() && !has_empty(&b) {
        for (t, f) in ladder {
            if f(&mut b) {
                steps[t as usize] += 1;
                hardest = Some(hardest.map_or(t, |h| h.max(t)));
                continue 'outer;
            }
        }
        break;
    }
    let difficulty = if !b.solved() || has_empty(&b) {
        Difficulty::Expert
    } else {
        match hardest {
            Some(t) if t >= Technique::HiddenPair => Difficulty::Hard,
            Some(t) if t >= Technique::LockedCandidates => Difficulty::Medium,
            _ => Difficulty::Easy,
        }
    };
    Grading { difficulty, hardest, steps }
}

/// Starting clue targets per tier; harder tiers remove more.
fn clue_target(d: Difficulty) -> usize {
    match d {
        Difficulty::Easy => 36,
        Difficulty::Medium => 30,
        Difficulty::Hard => 26,
        Difficulty::Expert => 22,
    }
}

/// Generate until the grade matches. Returns the best-effort puzzle after
/// `max_attempts` (so callers always get a valid unique puzzle).
pub fn generate_graded<R: Rng>(rng: &mut R, want: Difficulty, max_attempts: usize) -> (crate::sudoku::Puzzle, Grading) {
    let mut last = None;
    for _ in 0..max_attempts {
        let p = generate(rng, clue_target(want));
        let g = grade(&p.puzzle);
        if g.difficulty == want {
            return (p, g);
        }
        last = Some((p, g));
    }
    last.expect("max_attempts must be > 0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sudoku::{count_solutions, is_valid_complete, solve};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::time::Instant;

    fn parse(s: &str) -> Grid {
        let mut g = [0u8; 81];
        for (i, ch) in s.chars().enumerate() {
            g[i] = ch.to_digit(10).unwrap_or(0) as u8;
        }
        g
    }

    #[test]
    fn known_easy_puzzle_needs_only_singles() {
        let g = grade(&parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079"));
        assert_eq!(g.difficulty, Difficulty::Easy);
        assert!(g.hardest <= Some(Technique::HiddenSingle));
    }

    #[test]
    fn known_hard_puzzle_is_not_easy() {
        // Arto Inkala's 2012 puzzle: needs far more than singles.
        let p = parse("800000000003600000070090200050007000000045700000100030001000068008500010090000400");
        assert_eq!(count_solutions(&p, 2), 1);
        let g = grade(&p);
        assert!(g.difficulty >= Difficulty::Hard, "got {:?}", g);
    }

    #[test]
    fn grading_does_not_break_solutions() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..20 {
            let p = generate(&mut rng, 28);
            let g = grade(&p.puzzle);
            if g.difficulty != Difficulty::Expert {
                // Human-solvable means the unique solution is reachable.
                assert_eq!(count_solutions(&p.puzzle, 2), 1);
                assert!(is_valid_complete(&solve(&p.puzzle).unwrap()));
            }
        }
    }

    #[test]
    fn tiers_are_reachable_and_fast() {
        let mut rng = StdRng::seed_from_u64(11);
        for want in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert] {
            let mut hits = 0;
            let mut times = Vec::new();
            let n = 10;
            for _ in 0..n {
                let t = Instant::now();
                let (p, g) = generate_graded(&mut rng, want, 300);
                times.push(t.elapsed().as_millis());
                assert_eq!(count_solutions(&p.puzzle, 2), 1);
                if g.difficulty == want {
                    hits += 1;
                }
            }
            times.sort();
            println!("{:?}: matched {}/{} p50={}ms max={}ms", want, hits, n, times[n / 2], times[n - 1]);
            assert!(hits >= 8, "{:?} matched only {}/{}", want, hits, n);
        }
    }
}
