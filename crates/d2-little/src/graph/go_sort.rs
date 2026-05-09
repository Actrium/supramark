//! Port of Go's `sort.Slice` (pdqsort_func) so Rust-side sorts yield the
//! exact same ordering that the upstream d2 Go pipeline produces.
//!
//! Matches `src/sort/zsortfunc.go` in the Go toolchain (Go 1.22/1.24).
//! We only need the unstable entry point; `SliceStable` is unused here.
//!
//! The public entry point is [`go_sort_slice`]. The callback is the Go
//! comparator: `less(i, j) -> bool` returns whether element at index `i`
//! should sort before element at index `j`. Indices always refer to the
//! current (in-place) positions of the slice being sorted, mirroring
//! `sort.Slice`.
//!
//! When multiple edges / objects share identical sort keys, Go's
//! `sort.Slice` leaves them in a specific non-stable order that the
//! upstream e2e fixtures depend on. A stable Rust sort produces a
//! different order. To preserve byte-level SVG parity we port Go's
//! algorithm 1:1 (including `xorshift`-based pattern breaking).

/// Go's pdqsort bad-pivot limit (`bits.Len(uint(length))`), computed the
/// same way Go does — number of bits needed to represent the length.
fn initial_limit(n: usize) -> usize {
    // bits::Len(uint(n)) == 64 - n.leading_zeros() for n > 0, else 0.
    if n == 0 {
        0
    } else {
        (usize::BITS - (n as u64).leading_zeros()) as usize
    }
}

/// Go's `sort.Slice(x, less)` on a virtual range `[0, n)`. `less` receives
/// the current (in-place) indices and must never short-circuit side
/// effects; `swap` mutates the slice in place.
pub fn go_sort_slice<L, S>(n: usize, mut less: L, mut swap: S)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let limit = initial_limit(n);
    pdqsort(&mut less, &mut swap, 0, n, limit);
}

// -------------------------------------------------------------------------
// Straight port of Go's zsortfunc.go — kept structurally identical to
// simplify future maintenance. Function names use `_func` suffix like Go.

fn insertion_sort_func<L, S>(less: &mut L, swap: &mut S, a: usize, b: usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let mut i = a + 1;
    while i < b {
        let mut j = i;
        while j > a && less(j, j - 1) {
            swap(j, j - 1);
            j -= 1;
        }
        i += 1;
    }
}

fn sift_down_func<L, S>(less: &mut L, swap: &mut S, lo: usize, hi: usize, first: usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let mut root = lo;
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            break;
        }
        if child + 1 < hi && less(first + child, first + child + 1) {
            child += 1;
        }
        if !less(first + root, first + child) {
            return;
        }
        swap(first + root, first + child);
        root = child;
    }
}

fn heap_sort_func<L, S>(less: &mut L, swap: &mut S, a: usize, b: usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let first = a;
    let lo = 0usize;
    let hi = b - a;

    if hi > 1 {
        let mut i = (hi - 1) / 2 + 1;
        while i > 0 {
            i -= 1;
            sift_down_func(less, swap, i, hi, first);
        }
    }

    let mut i = hi;
    while i > 0 {
        i -= 1;
        swap(first, first + i);
        sift_down_func(less, swap, lo, i, first);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortedHint {
    Unknown,
    Increasing,
    Decreasing,
}

fn order2_func<L, S>(
    less: &mut L,
    _swap: &mut S,
    a: usize,
    b: usize,
    swaps: &mut usize,
) -> (usize, usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    if less(b, a) {
        *swaps += 1;
        (b, a)
    } else {
        (a, b)
    }
}

fn median_func<L, S>(
    less: &mut L,
    swap: &mut S,
    a: usize,
    b: usize,
    c: usize,
    swaps: &mut usize,
) -> usize
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let (a, b) = order2_func(less, swap, a, b, swaps);
    let (b, c) = order2_func(less, swap, b, c, swaps);
    let (_a, b) = order2_func(less, swap, a, b, swaps);
    let _ = c;
    b
}

fn median_adjacent_func<L, S>(less: &mut L, swap: &mut S, a: usize, swaps: &mut usize) -> usize
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    median_func(less, swap, a - 1, a, a + 1, swaps)
}

fn choose_pivot_func<L, S>(less: &mut L, swap: &mut S, a: usize, b: usize) -> (usize, SortedHint)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    const SHORTEST_NINTHER: usize = 50;
    const MAX_SWAPS: usize = 4 * 3;

    let l = b - a;
    let mut swaps = 0usize;
    let mut i = a + l / 4;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;

    if l >= 8 {
        if l >= SHORTEST_NINTHER {
            i = median_adjacent_func(less, swap, i, &mut swaps);
            j = median_adjacent_func(less, swap, j, &mut swaps);
            k = median_adjacent_func(less, swap, k, &mut swaps);
        }
        j = median_func(less, swap, i, j, k, &mut swaps);
    }

    match swaps {
        0 => (j, SortedHint::Increasing),
        x if x == MAX_SWAPS => (j, SortedHint::Decreasing),
        _ => (j, SortedHint::Unknown),
    }
}

fn reverse_range_func<S>(swap: &mut S, a: usize, b: usize)
where
    S: FnMut(usize, usize),
{
    let mut i = a;
    let mut j = b - 1;
    while i < j {
        swap(i, j);
        i += 1;
        j -= 1;
    }
}

#[allow(dead_code)]
fn swap_range_func<S>(swap: &mut S, a: usize, b: usize, n: usize)
where
    S: FnMut(usize, usize),
{
    for i in 0..n {
        swap(a + i, b + i);
    }
}

fn next_power_of_two(length: usize) -> usize {
    // Go: `uint(1 << bits.Len(uint(length)))`.
    if length == 0 {
        1
    } else {
        1usize << ((usize::BITS - (length as u64).leading_zeros()) as usize)
    }
}

/// Go's xorshift* — a fixed-seed PRNG that must produce the exact same
/// sequence for the same length, so `breakPatterns_func` scatters the
/// same indices on both platforms.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn break_patterns_func<L, S>(_less: &mut L, swap: &mut S, a: usize, b: usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    let length = b - a;
    if length >= 8 {
        let mut random = Xorshift(length as u64);
        let modulus = next_power_of_two(length);

        let start = a + (length / 4) * 2 - 1;
        let end = a + (length / 4) * 2 + 1;
        for idx in start..=end {
            let mut other = (random.next() as usize) & (modulus - 1);
            if other >= length {
                other -= length;
            }
            swap(idx, a + other);
        }
    }
}

fn partition_func<L, S>(
    less: &mut L,
    swap: &mut S,
    a: usize,
    b: usize,
    pivot: usize,
) -> (usize, bool)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    while i <= j && less(i, a) {
        i += 1;
    }
    while i <= j && !less(j, a) {
        if j == 0 {
            break;
        }
        j -= 1;
    }
    if i > j {
        swap(j, a);
        return (j, true);
    }
    swap(i, j);
    i += 1;
    if j == 0 {
        // Edge case: Go's loop relies on signed-ish semantics; we guard
        // against wrap-around. j==0 here means we're done.
        return (a, false);
    }
    j -= 1;

    loop {
        while i <= j && less(i, a) {
            i += 1;
        }
        while i <= j && !less(j, a) {
            if j == 0 {
                break;
            }
            j -= 1;
        }
        if i > j {
            break;
        }
        swap(i, j);
        i += 1;
        if j == 0 {
            break;
        }
        j -= 1;
    }
    swap(j, a);
    (j, false)
}

fn partition_equal_func<L, S>(less: &mut L, swap: &mut S, a: usize, b: usize, pivot: usize) -> usize
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    loop {
        while i <= j && !less(a, i) {
            i += 1;
        }
        while i <= j && less(a, j) {
            if j == 0 {
                break;
            }
            j -= 1;
        }
        if i > j {
            break;
        }
        swap(i, j);
        i += 1;
        if j == 0 {
            break;
        }
        j -= 1;
    }
    i
}

fn partial_insertion_sort_func<L, S>(less: &mut L, swap: &mut S, a: usize, b: usize) -> bool
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    const MAX_STEPS: usize = 5;
    const SHORTEST_SHIFTING: usize = 50;

    let mut i = a + 1;
    for _step in 0..MAX_STEPS {
        while i < b && !less(i, i - 1) {
            i += 1;
        }

        if i == b {
            return true;
        }

        if b - a < SHORTEST_SHIFTING {
            return false;
        }

        swap(i, i - 1);

        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !less(j, j - 1) {
                    break;
                }
                swap(j, j - 1);
                j -= 1;
            }
        }
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !less(j, j - 1) {
                    break;
                }
                swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}

fn pdqsort<L, S>(less: &mut L, swap: &mut S, mut a: usize, mut b: usize, mut limit: usize)
where
    L: FnMut(usize, usize) -> bool,
    S: FnMut(usize, usize),
{
    const MAX_INSERTION: usize = 12;
    let mut was_balanced = true;
    let mut was_partitioned = true;

    loop {
        let length = b - a;
        if length <= MAX_INSERTION {
            insertion_sort_func(less, swap, a, b);
            return;
        }

        if limit == 0 {
            heap_sort_func(less, swap, a, b);
            return;
        }

        if !was_balanced {
            break_patterns_func(less, swap, a, b);
            limit -= 1;
        }

        let (pivot, mut hint) = choose_pivot_func(less, swap, a, b);
        let mut pivot = pivot;
        if hint == SortedHint::Decreasing {
            reverse_range_func(swap, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = SortedHint::Increasing;
        }

        if was_balanced
            && was_partitioned
            && hint == SortedHint::Increasing
            && partial_insertion_sort_func(less, swap, a, b)
        {
            return;
        }

        if a > 0 && !less(a - 1, pivot) {
            let mid = partition_equal_func(less, swap, a, b, pivot);
            a = mid;
            continue;
        }

        let (mid, already_partitioned) = partition_func(less, swap, a, b, pivot);
        was_partitioned = already_partitioned;

        let left_len = mid - a;
        let right_len = b - mid;
        let balance_threshold = length / 8;
        if left_len < right_len {
            was_balanced = left_len >= balance_threshold;
            pdqsort(less, swap, a, mid, limit);
            a = mid + 1;
        } else {
            was_balanced = right_len >= balance_threshold;
            pdqsort(less, swap, mid + 1, b, limit);
            b = mid;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replicates the Go-specific reordering that drives the
    /// `stable/nesting_power` e2e fixture: three equal-key entries
    /// (`dagre_layout`, `elk_layout`, `tala_layout`) embedded in a
    /// 94-element slice must end up as `tala, dagre, elk` to match Go's
    /// `sort.Slice`.
    #[test]
    fn matches_go_pdqsort_for_nesting_power_inputs_rust_layout() {
        // Rust-side layout: the three glob edges are appended at the end
        // (positions 91-93) rather than Go's positions 18-20. Either way,
        // Go's `sort.Slice` produces `tala, dagre, elk`.
        let linemap: std::collections::HashMap<u32, u32> = [
            (20, 78),
            (21, 78),
            (22, 79),
            (23, 79),
            (24, 80),
            (25, 80),
            (26, 81),
            (27, 81),
            (28, 82),
            (29, 82),
            (30, 83),
            (31, 93),
            (32, 93),
            (33, 94),
            (34, 98),
            (35, 98),
            (36, 99),
            (37, 106),
            (38, 106),
            (39, 107),
            (40, 111),
            (41, 111),
            (42, 112),
            (43, 123),
            (44, 123),
            (45, 123),
            (46, 123),
            (47, 141),
            (48, 144),
            (49, 145),
            (50, 146),
            (51, 147),
            (52, 148),
            (53, 150),
            (54, 152),
            (55, 153),
            (56, 183),
            (57, 199),
            (58, 200),
            (59, 202),
            (60, 203),
            (61, 205),
            (62, 206),
            (63, 207),
            (64, 208),
            (65, 236),
            (66, 237),
            (67, 238),
            (68, 239),
            (69, 240),
            (70, 241),
            (71, 242),
            (72, 243),
            (73, 211),
            (74, 212),
            (75, 213),
            (76, 233),
            (77, 192),
            (78, 57),
            (79, 127),
            (80, 156),
            (81, 247),
            (82, 248),
            (83, 249),
            (84, 250),
            (85, 251),
            (86, 252),
            (87, 254),
            (88, 255),
            (89, 256),
            (90, 262),
        ]
        .iter()
        .copied()
        .collect();
        let mut edges: Vec<(String, u32, u32)> = vec![
            ("dagre_elk".into(), 8, 5),
            ("elk_tala".into(), 8, 14),
            ("l.here.this.row1_row2".into(), 19, 7),
            ("l.here.this.row2_row3".into(), 19, 16),
            ("l.here.this.row3_row4".into(), 19, 25),
            ("l.here.this.row4_row5".into(), 19, 34),
            ("grid.you can.grid.1_2".into(), 40, 11),
            ("grid.you can.grid.2_3".into(), 40, 16),
            ("grid.continue_nesting".into(), 45, 11),
            ("grid.with_a".into(), 27, 7),
            ("grid.a_x".into(), 27, 15),
            ("grid.x_you can".into(), 27, 20),
            ("grid.nesting_x".into(), 48, 7),
            ("here.this_is".into(), 15, 5),
            ("here.is_grid".into(), 22, 5),
            ("here.row2_grandchild".into(), 50, 5),
            ("l.default_layout".into(), 5, 3),
            ("l.layout_here".into(), 5, 14),
            ("row5_dagre".into(), 53, 3),
            ("row1_tala".into(), 54, 3),
        ];
        for i in 20u32..91 {
            edges.push((format!("edge{i}"), linemap[&i], 0));
        }
        edges.push(("dagre_layout".into(), 10, 3));
        edges.push(("elk_layout".into(), 10, 3));
        edges.push(("tala_layout".into(), 10, 3));

        let n = edges.len();
        use std::cell::RefCell;
        let edges_cell = RefCell::new(edges);
        go_sort_slice(
            n,
            |i, j| {
                let e = edges_cell.borrow();
                if e[i].1 != e[j].1 {
                    e[i].1 < e[j].1
                } else {
                    e[i].2 < e[j].2
                }
            },
            |i, j| {
                edges_cell.borrow_mut().swap(i, j);
            },
        );
        let edges = edges_cell.into_inner();
        let glob_order: Vec<&str> = edges
            .iter()
            .filter(|e| e.1 == 10 && e.2 == 3)
            .map(|e| e.0.as_str())
            .collect();
        assert_eq!(
            glob_order,
            vec!["tala_layout", "dagre_layout", "elk_layout"],
            "rust-layout input must still sort to `tala, dagre, elk`"
        );
    }

    #[test]
    fn matches_go_pdqsort_for_nesting_power_inputs() {
        // Build the same 94-element input the d2 compiler feeds to
        // SortEdgesByAST for stable/nesting_power. (name, line, col)
        let mut edges: Vec<(String, u32, u32)> = vec![
            ("dagre_elk".into(), 8, 5),
            ("elk_tala".into(), 8, 14),
            ("l.here.this.row1_row2".into(), 19, 7),
            ("l.here.this.row2_row3".into(), 19, 16),
            ("l.here.this.row3_row4".into(), 19, 25),
            ("l.here.this.row4_row5".into(), 19, 34),
            ("grid.you can.grid.1_2".into(), 40, 11),
            ("grid.you can.grid.2_3".into(), 40, 16),
            ("grid.continue_nesting".into(), 45, 11),
            ("grid.with_a".into(), 27, 7),
            ("grid.a_x".into(), 27, 15),
            ("grid.x_you can".into(), 27, 20),
            ("grid.nesting_x".into(), 48, 7),
            ("here.this_is".into(), 15, 5),
            ("here.is_grid".into(), 22, 5),
            ("here.row2_grandchild".into(), 50, 5),
            ("l.default_layout".into(), 5, 3),
            ("l.layout_here".into(), 5, 14),
            ("dagre_layout".into(), 10, 3),
            ("elk_layout".into(), 10, 3),
            ("tala_layout".into(), 10, 3),
            ("row5_dagre".into(), 53, 3),
            ("row1_tala".into(), 54, 3),
        ];
        for i in 23..94 {
            edges.push((format!("edge{i}"), (100 + i) as u32, 0));
        }

        let n = edges.len();
        use std::cell::RefCell;
        let edges_cell = RefCell::new(edges);
        go_sort_slice(
            n,
            |i, j| {
                let e = edges_cell.borrow();
                if e[i].1 != e[j].1 {
                    e[i].1 < e[j].1
                } else {
                    e[i].2 < e[j].2
                }
            },
            |i, j| {
                edges_cell.borrow_mut().swap(i, j);
            },
        );
        let edges = edges_cell.into_inner();

        let glob_order: Vec<&str> = edges
            .iter()
            .filter(|e| e.1 == 10 && e.2 == 3)
            .map(|e| e.0.as_str())
            .collect();
        assert_eq!(
            glob_order,
            vec!["tala_layout", "dagre_layout", "elk_layout"],
            "glob-expansion order must match Go pdqsort output"
        );
    }
}
