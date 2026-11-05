// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Facts about the input LLVM IR that lowering consults.
//!
//! Analyses are queries: they do not rewrite IR and know nothing about GOTO. Each one only ever
//! proves a property (e.g. that an operation cannot wrap); an inconclusive answer must leave the
//! lowering as it would be without the analysis, so a weak analysis costs performance, not
//! soundness.

use std::collections::{BTreeMap, BTreeSet};

/// Permutation of the blocks `0..succs.len()`, given in RPO with their successor lists, that keeps
/// each natural loop contiguous with its header first (see `blocks_in_lowering_order`).
pub(super) fn loop_contiguous_order(succs: &[Vec<usize>]) -> Vec<usize> {
    let n = succs.len();
    let mut preds = vec![Vec::new(); n];
    for (u, ss) in succs.iter().enumerate() {
        for &s in ss {
            preds[s].push(u);
        }
    }
    // Natural loop of header `h`: `h` plus every block reaching a retreating edge `u -> h`
    // without passing `h`. Restricting to RPO index `>= h` bounds irreducible regions.
    let mut loops: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (u, ss) in succs.iter().enumerate() {
        // In RPO, `u -> h` with `h <= u` is retreating: a back edge to header `h` if reducible.
        for &h in ss.iter().filter(|&&h| h <= u) {
            let body = loops.entry(h).or_insert_with(|| BTreeSet::from([h]));
            // Walk predecessors back from the edge source; `h` is already in `body`, so stops there.
            let mut work = vec![u];
            while let Some(b) = work.pop() {
                if b >= h && body.insert(b) {
                    work.extend(&preds[b]);
                }
            }
        }
    }

    fn place(
        region: impl Iterator<Item = usize>,
        loops: &BTreeMap<usize, BTreeSet<usize>>,
        placed: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        for b in region {
            if placed[b] {
                continue;
            }
            placed[b] = true;
            order.push(b);
            // A header has the smallest RPO index in its loop, so it is reached before its body:
            // emit the rest of the loop right after it (recursively, so inner loops nest).
            if let Some(body) = loops.get(&b) {
                place(body.iter().copied(), loops, placed, order);
            }
        }
    }
    let mut placed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    place(0..n, &loops, &mut placed, &mut order);
    order
}

#[cfg(test)]
mod tests {
    use super::loop_contiguous_order;

    #[test]
    fn acyclic_keeps_rpo() {
        // Diamond: 0 -> {1, 2} -> 3.
        assert_eq!(
            loop_contiguous_order(&[vec![1, 2], vec![3], vec![3], vec![]]),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn nested_loop_body_moves_before_outer_latch() {
        // loop_nest_unwind.ll in RPO: entry, outer.header, inner.header, outer.latch,
        // inner.body, inner.latch, exit. The inner loop {2, 4, 5} must not contain 3.
        let succs = [
            vec![1, 6], // 0 entry
            vec![2],    // 1 outer.header
            vec![4, 3], // 2 inner.header
            vec![1, 6], // 3 outer.latch
            vec![5],    // 4 inner.body
            vec![2],    // 5 inner.latch
            vec![],     // 6 exit
        ];
        assert_eq!(loop_contiguous_order(&succs), [0, 1, 2, 4, 5, 3, 6]);
    }

    #[test]
    fn self_loop_and_sibling_loops_keep_rpo() {
        // 0 -> 1 (self loop) -> 2 <-> 3 -> 4.
        let succs = [vec![1], vec![1, 2], vec![3], vec![2, 4], vec![]];
        assert_eq!(loop_contiguous_order(&succs), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn irreducible_cycle_is_a_permutation() {
        // Two entries into the cycle 1 <-> 2; 2 -> 1 is retreating but 1 does not dominate 2.
        let succs = [vec![1, 2], vec![2, 3], vec![1], vec![]];
        assert_eq!(loop_contiguous_order(&succs), [0, 1, 2, 3]);
    }

    #[test]
    fn loop_exit_inside_rpo_range_moves_after_body() {
        // 0 -> 1 header; 1 -> {2 exit, 3 body}; 3 -> 1. RPO may list the exit first.
        let succs = [vec![1], vec![2, 3], vec![], vec![1]];
        assert_eq!(loop_contiguous_order(&succs), [0, 1, 3, 2]);
    }
}
