//! Sudoku grid, bitmask solver (MRV backtracking), and unique-solution generator.

use rand::seq::SliceRandom;
use rand::Rng;

pub type Grid = [u8; 81];

const ALL: u16 = 0b1_1111_1111;

fn row(i: usize) -> usize {
    i / 9
}
fn col(i: usize) -> usize {
    i % 9
}
fn boxi(i: usize) -> usize {
    (row(i) / 3) * 3 + col(i) / 3
}

#[derive(Clone)]
struct State {
    grid: Grid,
    rows: [u16; 9],
    cols: [u16; 9],
    boxes: [u16; 9],
}

impl State {
    /// Returns None if the givens already conflict.
    fn new(grid: &Grid) -> Option<State> {
        let mut s = State { grid: [0; 81], rows: [0; 9], cols: [0; 9], boxes: [0; 9] };
        for (i, &v) in grid.iter().enumerate() {
            if v != 0 {
                if v > 9 {
                    return None;
                }
                let bit = 1u16 << (v - 1);
                if (s.rows[row(i)] | s.cols[col(i)] | s.boxes[boxi(i)]) & bit != 0 {
                    return None;
                }
                s.place(i, v);
            }
        }
        Some(s)
    }

    fn place(&mut self, i: usize, v: u8) {
        let bit = 1u16 << (v - 1);
        self.grid[i] = v;
        self.rows[row(i)] |= bit;
        self.cols[col(i)] |= bit;
        self.boxes[boxi(i)] |= bit;
    }

    fn unplace(&mut self, i: usize, v: u8) {
        let bit = !(1u16 << (v - 1));
        self.grid[i] = 0;
        self.rows[row(i)] &= bit;
        self.cols[col(i)] &= bit;
        self.boxes[boxi(i)] &= bit;
    }

    fn candidates(&self, i: usize) -> u16 {
        !(self.rows[row(i)] | self.cols[col(i)] | self.boxes[boxi(i)]) & ALL
    }

    /// Empty cell with fewest candidates (MRV); None if grid is full.
    fn best_cell(&self) -> Option<(usize, u16)> {
        let mut best: Option<(usize, u16)> = None;
        let mut best_n = 10;
        for i in 0..81 {
            if self.grid[i] == 0 {
                let c = self.candidates(i);
                let n = c.count_ones();
                if n < best_n {
                    best_n = n;
                    best = Some((i, c));
                    if n <= 1 {
                        break;
                    }
                }
            }
        }
        best
    }
}

fn search(s: &mut State, limit: usize, found: &mut usize, first: &mut Option<Grid>, rng: &mut Option<&mut dyn rand::RngCore>) {
    let Some((i, cands)) = s.best_cell() else {
        if first.is_none() {
            *first = Some(s.grid);
        }
        *found += 1;
        return;
    };
    let mut vals: Vec<u8> = (1..=9).filter(|v| cands & (1 << (v - 1)) != 0).collect();
    if let Some(r) = rng.as_mut() {
        vals.shuffle(r);
    }
    for v in vals {
        s.place(i, v);
        search(s, limit, found, first, rng);
        s.unplace(i, v);
        if *found >= limit {
            return;
        }
    }
}

/// Counts solutions up to `limit`.
pub fn count_solutions(grid: &Grid, limit: usize) -> usize {
    let Some(mut s) = State::new(grid) else { return 0 };
    let mut found = 0;
    search(&mut s, limit, &mut found, &mut None, &mut None);
    found
}

pub fn solve(grid: &Grid) -> Option<Grid> {
    let mut s = State::new(grid)?;
    let (mut found, mut first) = (0, None);
    search(&mut s, 1, &mut found, &mut first, &mut None);
    first
}

/// A random fully solved grid.
pub fn random_solution<R: Rng>(rng: &mut R) -> Grid {
    let mut s = State::new(&[0; 81]).unwrap();
    let (mut found, mut first) = (0, None);
    let mut r: Option<&mut dyn rand::RngCore> = Some(rng);
    search(&mut s, 1, &mut found, &mut first, &mut r);
    first.unwrap()
}

pub struct Puzzle {
    pub puzzle: Grid,
    pub solution: Grid,
}

/// Remove cells (180° symmetric pairs) while the solution stays unique,
/// until at most `target_clues` remain or no more can be removed.
pub fn generate<R: Rng>(rng: &mut R, target_clues: usize) -> Puzzle {
    let solution = random_solution(rng);
    let mut puzzle = solution;
    let mut order: Vec<usize> = (0..41).collect();
    order.shuffle(rng);
    let mut clues = 81;
    for i in order {
        if clues <= target_clues {
            break;
        }
        let j = 80 - i;
        let (a, b) = (puzzle[i], puzzle[j]);
        puzzle[i] = 0;
        puzzle[j] = 0;
        if count_solutions(&puzzle, 2) == 1 {
            clues -= if i == j { 1 } else { 2 };
        } else {
            puzzle[i] = a;
            puzzle[j] = b;
        }
    }
    Puzzle { puzzle, solution }
}

pub fn is_valid_complete(grid: &Grid) -> bool {
    let mut s = State { grid: [0; 81], rows: [0; 9], cols: [0; 9], boxes: [0; 9] };
    for (i, &v) in grid.iter().enumerate() {
        if !(1..=9).contains(&v) {
            return false;
        }
        let bit = 1u16 << (v - 1);
        if (s.rows[row(i)] | s.cols[col(i)] | s.boxes[boxi(i)]) & bit != 0 {
            return false;
        }
        s.place(i, v);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::time::Instant;

    #[test]
    fn solution_is_valid() {
        let mut rng = StdRng::seed_from_u64(1);
        assert!(is_valid_complete(&random_solution(&mut rng)));
    }

    #[test]
    fn generated_puzzles_are_unique_and_consistent() {
        let mut rng = StdRng::seed_from_u64(42);
        for target in [40, 32, 26] {
            let p = generate(&mut rng, target);
            assert_eq!(count_solutions(&p.puzzle, 2), 1);
            assert_eq!(solve(&p.puzzle), Some(p.solution));
            for i in 0..81 {
                assert!(p.puzzle[i] == 0 || p.puzzle[i] == p.solution[i]);
            }
        }
    }

    #[test]
    fn rejects_conflicting_givens() {
        let mut g = [0u8; 81];
        g[0] = 5;
        g[1] = 5;
        assert_eq!(count_solutions(&g, 2), 0);
    }

    #[test]
    fn empty_grid_has_multiple_solutions() {
        assert_eq!(count_solutions(&[0; 81], 2), 2);
    }

    #[test]
    fn bench_generation() {
        let mut rng = StdRng::seed_from_u64(7);
        let n = 50;
        let mut times = Vec::new();
        for _ in 0..n {
            let t = Instant::now();
            let _ = generate(&mut rng, 28);
            times.push(t.elapsed().as_millis());
        }
        times.sort();
        println!("sudoku generate(28 clues): p50={}ms p95={}ms max={}ms", times[n / 2], times[n * 95 / 100], times[n - 1]);
    }
}
