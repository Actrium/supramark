//! Sort resolved entries by barycenter, interleaving unsortable entries.
//!
//! Entries that have a barycenter are sorted by it. Entries without a barycenter
//! ("unsortable") are placed at their original index position, interleaved
//! among the sorted entries.

use super::resolve_conflicts::ResolvedEntry;

/// Result of sorting: the final node order and optional aggregate barycenter/weight.
#[derive(Debug, Clone)]
pub(crate) struct SortResult {
    pub vs: Vec<String>,
    pub barycenter: Option<f64>,
    pub weight: Option<i32>,
}

/// Sort entries, placing those with barycenters in order and interleaving
/// those without at their original positions.
pub(crate) fn sort(entries: &[ResolvedEntry], bias_right: bool) -> SortResult {
    // Partition into sortable (has barycenter) and unsortable (no barycenter)
    let mut sortable: Vec<&ResolvedEntry> = Vec::new();
    let mut unsortable: Vec<&ResolvedEntry> = Vec::new();

    for entry in entries {
        if entry.barycenter.is_some() {
            sortable.push(entry);
        } else {
            unsortable.push(entry);
        }
    }

    // Sort unsortable by descending index (so we can pop from the end to get ascending)
    unsortable.sort_by_key(|entry| std::cmp::Reverse(entry.i));

    // Sort sortable by barycenter, with tie-breaking by index
    sortable.sort_by(|a, b| {
        let a_bc = a.barycenter.unwrap();
        let b_bc = b.barycenter.unwrap();
        if a_bc < b_bc {
            return std::cmp::Ordering::Less;
        }
        if a_bc > b_bc {
            return std::cmp::Ordering::Greater;
        }
        // Tie-break by index
        if !bias_right {
            a.i.cmp(&b.i)
        } else {
            b.i.cmp(&a.i)
        }
    });

    let mut vs: Vec<Vec<String>> = Vec::new();
    let mut sum = 0.0_f64;
    let mut weight = 0_i32;
    let mut vs_index: usize = 0;

    // Consume unsortable entries whose index <= current position
    consume_unsortable(&mut vs, &mut unsortable, &mut vs_index);

    for entry in &sortable {
        vs_index += entry.vs.len();
        vs.push(entry.vs.clone());
        sum += entry.barycenter.unwrap() * entry.weight.unwrap() as f64;
        weight += entry.weight.unwrap();
        consume_unsortable(&mut vs, &mut unsortable, &mut vs_index);
    }

    let flat: Vec<String> = vs.into_iter().flatten().collect();

    let mut result = SortResult {
        vs: flat,
        barycenter: None,
        weight: None,
    };

    if weight > 0 {
        result.barycenter = Some(sum / weight as f64);
        result.weight = Some(weight);
    }

    result
}

/// Pop unsortable entries whose original index is <= the current vs_index.
fn consume_unsortable(
    vs: &mut Vec<Vec<String>>,
    unsortable: &mut Vec<&ResolvedEntry>,
    index: &mut usize,
) {
    while let Some(last) = unsortable.last() {
        if last.i <= *index {
            let entry = unsortable.pop().unwrap();
            vs.push(entry.vs.clone());
            *index += 1;
        } else {
            break;
        }
    }
}
