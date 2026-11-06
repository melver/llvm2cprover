// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Read-only queries about the input LLVM IR that lowering consults.
//!
//! Queries do not rewrite IR and know nothing about GOTO, so each is unit-testable in isolation.
//! Some fix structure lowering depends on (block order); the rest only prove properties (e.g. that
//! an operation cannot wrap), and an inconclusive answer must leave the lowering as it would be
//! without the analysis, so a weak analysis costs performance, not soundness.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use inkwell::basic_block::BasicBlock;
use inkwell::llvm_sys::LLVMOpcode;
use inkwell::llvm_sys::LLVMTypeKind;
use inkwell::llvm_sys::core::{
    LLVMConstIntGetSExtValue, LLVMConstIntGetZExtValue, LLVMConstNull, LLVMCountIncoming,
    LLVMGetAggregateElement, LLVMGetBasicBlockTerminator, LLVMGetCalledValue, LLVMGetElementType,
    LLVMGetIncomingValue, LLVMGetInstructionOpcode, LLVMGetIntTypeWidth, LLVMGetIntrinsicID,
    LLVMGetNumSuccessors, LLVMGetOperand, LLVMGetSuccessor, LLVMGetTypeKind, LLVMGetVectorSize,
    LLVMIsAConstantAggregateZero, LLVMIsAConstantDataVector, LLVMIsAConstantInt,
    LLVMIsAConstantVector, LLVMIsAFunction, LLVMIsAInstruction, LLVMLookupIntrinsicID, LLVMTypeOf,
};
use inkwell::llvm_sys::prelude::{LLVMBasicBlockRef, LLVMValueRef};
use inkwell::values::{AsValueRef, FunctionValue, InstructionValue};

/// The reachable blocks of `function` in RPO, rearranged so that each natural loop is contiguous
/// with its header first. This is the order blocks are lowered in.
///
/// CBMC identifies loops by backward `GOTO`s and needs their ranges to nest. Plain RPO can
/// place an outer loop's latch inside an inner loop's range, and CBMC then counts inner
/// iterations against the outer loop. Moving a loop's body up to its header keeps the RPO
/// property `blocks_in_rpo` relies on, because the header dominates the whole body.
pub(super) fn lowering_order(function: FunctionValue<'_>) -> Vec<BasicBlock<'_>> {
    let rpo = blocks_in_rpo(function);
    let index: HashMap<BasicBlock<'_>, usize> =
        rpo.iter().enumerate().map(|(i, &bb)| (bb, i)).collect();
    let succs: Vec<Vec<usize>> = rpo
        .iter()
        .map(|bb| {
            let term = unsafe { LLVMGetBasicBlockTerminator(bb.as_mut_ptr()) };
            let n_succs = if term.is_null() {
                0
            } else {
                unsafe { LLVMGetNumSuccessors(term) }
            };
            (0..n_succs)
                .filter_map(|i| unsafe { BasicBlock::new(LLVMGetSuccessor(term, i)) })
                .filter_map(|succ| index.get(&succ).copied())
                .collect()
        })
        .collect();
    loop_contiguous_order(&succs)
        .into_iter()
        .map(|i| rpo[i])
        .collect()
}

/// The reachable blocks of `function` in reverse post-order (RPO).
///
/// RPO guarantees every dominator is visited before the blocks it dominates.
/// That is required so that when lowering resolves an incoming `phi` value
/// `[%v, %pred]`, the definition of `%v` has already been lowered. The IR
/// textual order produced by the optimizer may place loop-exit blocks before
/// the loop body, breaking the assumption that definitions precede their uses
/// in block order.
fn blocks_in_rpo(function: FunctionValue<'_>) -> Vec<BasicBlock<'_>> {
    let entry = match function.get_first_basic_block() {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut post_order: Vec<LLVMBasicBlockRef> = Vec::new();
    let mut visited: HashSet<LLVMBasicBlockRef> = HashSet::new();

    // Stack entries: (block, terminator, next-successor-index).
    // Storing the terminator avoids re-fetching it on every iteration.
    // Marking blocks visited at push-time prevents them from being pushed twice.
    let entry_ptr = entry.as_mut_ptr();
    let entry_term = unsafe { LLVMGetBasicBlockTerminator(entry_ptr) };
    visited.insert(entry_ptr);
    let mut stack: Vec<(LLVMBasicBlockRef, LLVMValueRef, u32)> = vec![(entry_ptr, entry_term, 0)];

    while let Some((bb, term, next_succ)) = stack.last_mut() {
        let (bb, term) = (*bb, *term);
        let n_succs = if term.is_null() {
            0
        } else {
            unsafe { LLVMGetNumSuccessors(term) }
        };
        let idx = *next_succ;
        if idx < n_succs {
            *next_succ += 1;
            let succ = unsafe { LLVMGetSuccessor(term, idx) };
            if visited.insert(succ) {
                let succ_term = unsafe { LLVMGetBasicBlockTerminator(succ) };
                stack.push((succ, succ_term, 0));
            }
        } else {
            post_order.push(bb);
            stack.pop();
        }
    }

    // RPO = reverse of post-order.  Map raw refs back to inkwell wrappers.
    post_order
        .into_iter()
        .rev()
        .map(|bb| unsafe { BasicBlock::new(bb).expect("valid basic block") })
        .collect()
}

/// Permutation of the blocks `0..succs.len()`, given in RPO with their successor lists, that keeps
/// each natural loop contiguous with its header first (see [`lowering_order`]).
fn loop_contiguous_order(succs: &[Vec<usize>]) -> Vec<usize> {
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

/// Whether `instruction` (`add`/`sub`/`mul`/`shl`) can wrap in the signed (`nsw`) and unsigned
/// (`nuw`) interpretation, i.e. whether that flag can make its result `poison`. Each is `false`
/// only when operand ranges prove it cannot, as for C arithmetic on integer-promoted narrow values.
pub(super) fn may_wrap(instruction: InstructionValue<'_>) -> (bool, bool) {
    let value = instruction.as_value_ref();
    let width = int_width(value);
    if width == 0 || width > MAX_WIDTH {
        return (true, true);
    }
    let (lhs, rhs) = unsafe { (LLVMGetOperand(value, 0), LLVMGetOperand(value, 1)) };
    let mut analysis = Ranges::default();
    let (a, b) = (analysis.of(lhs, MAX_DEPTH), analysis.of(rhs, MAX_DEPTH));
    // An empty range (a loop phi nothing outside the loop feeds) proves nothing.
    if a.is_empty() || b.is_empty() {
        return (true, true);
    }
    let exact = match unsafe { LLVMGetInstructionOpcode(value) } {
        LLVMOpcode::LLVMAdd => Range::add(a, b, width),
        LLVMOpcode::LLVMSub => Range::sub(a, b, width),
        LLVMOpcode::LLVMMul => Range::mul(a, b, width),
        LLVMOpcode::LLVMShl => match shift_counts(rhs, width) {
            Some(counts) => Range::shl(a, counts, width),
            None => return (true, true),
        },
        _ => return (true, true),
    };
    // `exact` holds the parts of the mathematically exact result that fit `width`.
    (exact.signed.is_none(), exact.unsigned.is_none())
}

/// The instruction right after `instruction`, if it is the only user of `instruction`'s result.
/// Nothing runs between the two, so the result can be recomputed there from unchanged operands.
pub(super) fn sole_user_is_next(instruction: InstructionValue<'_>) -> Option<InstructionValue<'_>> {
    let next = instruction.get_next_instruction()?;
    let only = instruction.get_first_use()?;
    (only.get_next_use().is_none() && only.get_user().as_value_ref() == next.as_value_ref())
        .then_some(next)
}

/// Widest integer the analysis models (ranges are `i128`/`u128`).
const MAX_WIDTH: u32 = 128;

/// Instructions looked through per query. Results are memoized, so this bounds recursion depth,
/// not work; it is deep enough for unrolled accumulations such as a 16-tap sum.
const MAX_DEPTH: u32 = 64;

/// Fixpoint iterations per loop `phi` before it is given up as unbounded. Values that do not grow
/// around the loop (e.g. re-loaded or masked each iteration) settle in two.
const PHI_ITERATIONS: u32 = 4;

/// The possible values of an integer (over all lanes of a vector), in both interpretations of its
/// bits: two's complement and unsigned. Either is the full range of the width when nothing is
/// known; both describe the same bit patterns, so either may sharpen the other.
///
/// Ranges describe the value when it is not `poison`. A `poison` operand makes every result that
/// depends on it `poison`, and any lowering of `poison` refines it, so a range that does not hold
/// for a `poison` operand never makes a check unsound to drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    slo: i128,
    shi: i128,
    ulo: u128,
    uhi: u128,
}

/// The exact result of an operation, in each interpretation in which it fits the width (`None`
/// where it may not, i.e. where the wrapped result can differ).
struct Exact {
    signed: Option<(i128, i128)>,
    unsigned: Option<(u128, u128)>,
}

impl Exact {
    /// The range of the wrapped result: exact where it fits, else unknown.
    fn range(self, width: u32) -> Range {
        let mut range = Range::full(width);
        if let Some((lo, hi)) = self.signed {
            range = range.meet(Range::from_signed(lo, hi, width));
        }
        if let Some((lo, hi)) = self.unsigned {
            range = range.meet(Range::from_unsigned(lo, hi, width));
        }
        range
    }
}

fn smin_of(width: u32) -> i128 {
    i128::MIN >> (MAX_WIDTH - width)
}

fn smax_of(width: u32) -> i128 {
    i128::MAX >> (MAX_WIDTH - width)
}

fn umax_of(width: u32) -> u128 {
    u128::MAX >> (MAX_WIDTH - width)
}

impl Range {
    /// No value: the identity of [`Range::join`], and what operations on it produce.
    const EMPTY: Range = Range {
        slo: i128::MAX,
        shi: i128::MIN,
        ulo: u128::MAX,
        uhi: 0,
    };

    fn is_empty(self) -> bool {
        self.slo > self.shi || self.ulo > self.uhi
    }

    fn full(width: u32) -> Range {
        Range {
            slo: smin_of(width),
            shi: smax_of(width),
            ulo: 0,
            uhi: umax_of(width),
        }
    }

    /// Values `lo..=hi` in two's complement, and what that implies for their unsigned bits.
    fn from_signed(lo: i128, hi: i128, width: u32) -> Range {
        let unsigned = |v: i128| {
            if v >= 0 {
                v as u128
            } else {
                (v as u128) & umax_of(width)
            }
        };
        let (ulo, uhi) = if lo >= 0 || hi < 0 {
            // One sign: the unsigned values are contiguous too.
            (unsigned(lo), unsigned(hi))
        } else {
            (0, umax_of(width))
        };
        Range {
            slo: lo,
            shi: hi,
            ulo,
            uhi,
        }
    }

    /// Unsigned values `lo..=hi`, and what that implies for their two's complement.
    fn from_unsigned(lo: u128, hi: u128, width: u32) -> Range {
        let smax = smax_of(width) as u128;
        let signed = |v: u128| {
            if v <= smax {
                v as i128
            } else {
                // v - 2^width, sign-extended from `width` bits.
                ((v << (MAX_WIDTH - width)) as i128) >> (MAX_WIDTH - width)
            }
        };
        let (slo, shi) = if hi <= smax || lo > smax {
            (signed(lo), signed(hi))
        } else {
            (smin_of(width), smax_of(width))
        };
        Range {
            slo,
            shi,
            ulo: lo,
            uhi: hi,
        }
    }

    /// Values within both `self` and `other` (two descriptions of the same value).
    fn meet(self, other: Range) -> Range {
        Range {
            slo: self.slo.max(other.slo),
            shi: self.shi.min(other.shi),
            ulo: self.ulo.max(other.ulo),
            uhi: self.uhi.min(other.uhi),
        }
    }

    /// Values within `self` or `other`.
    fn join(self, other: Range) -> Range {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Range {
            slo: self.slo.min(other.slo),
            shi: self.shi.max(other.shi),
            ulo: self.ulo.min(other.ulo),
            uhi: self.uhi.max(other.uhi),
        }
    }

    fn within(self, other: Range) -> bool {
        self.is_empty()
            || (other.slo <= self.slo
                && self.shi <= other.shi
                && other.ulo <= self.ulo
                && self.uhi <= other.uhi)
    }

    /// Bits a non-negative or two's complement value of the range needs.
    fn signed_bits(self) -> u32 {
        let bits = |v: i128| 128 - (if v < 0 { !v } else { v }).leading_zeros() + 1;
        bits(self.slo).max(bits(self.shi))
    }

    fn unsigned_bits(self) -> u32 {
        128 - self.uhi.leading_zeros()
    }

    /// Exact `a op b` where it fits `width`, from interval arithmetic.
    fn exact(signed: Option<(i128, i128)>, unsigned: Option<(u128, u128)>, width: u32) -> Exact {
        Exact {
            signed: signed.filter(|&(lo, hi)| smin_of(width) <= lo && hi <= smax_of(width)),
            unsigned: unsigned.filter(|&(_, hi)| hi <= umax_of(width)),
        }
    }

    fn add(a: Range, b: Range, width: u32) -> Exact {
        Range::exact(
            a.slo.checked_add(b.slo).zip(a.shi.checked_add(b.shi)),
            a.ulo.checked_add(b.ulo).zip(a.uhi.checked_add(b.uhi)),
            width,
        )
    }

    fn sub(a: Range, b: Range, width: u32) -> Exact {
        Range::exact(
            a.slo.checked_sub(b.shi).zip(a.shi.checked_sub(b.slo)),
            // Unsigned subtraction wraps unless the minuend is at least the subtrahend.
            (a.ulo >= b.uhi).then(|| (a.ulo - b.uhi, a.uhi - b.ulo)),
            width,
        )
    }

    fn mul(a: Range, b: Range, width: u32) -> Exact {
        let corners = [
            (a.slo, b.slo),
            (a.slo, b.shi),
            (a.shi, b.slo),
            (a.shi, b.shi),
        ]
        .map(|(x, y)| x.checked_mul(y));
        let signed = corners
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .map(|products| {
                let lo = products.iter().copied().min().unwrap_or(0);
                let hi = products.iter().copied().max().unwrap_or(0);
                (lo, hi)
            });
        Range::exact(
            signed,
            a.ulo.checked_mul(b.ulo).zip(a.uhi.checked_mul(b.uhi)),
            width,
        )
    }

    /// `a << c` for each `c` in `counts` (all below `width`): multiplication by `2^c`.
    fn shl(a: Range, (cmin, cmax): (u32, u32), width: u32) -> Exact {
        // 2^c, for the counts whose power of two the type can hold.
        let spow = |c: u32| (c < 127).then(|| 1_i128 << c);
        let upow = |c: u32| (c < 128).then(|| 1_u128 << c);
        let scale = |v: i128, c: u32| spow(c).and_then(|p| v.checked_mul(p));
        let signed = [
            scale(a.slo, cmin),
            scale(a.slo, cmax),
            scale(a.shi, cmin),
            scale(a.shi, cmax),
        ]
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .map(|v| {
            (
                v.iter().copied().min().unwrap_or(0),
                v.iter().copied().max().unwrap_or(0),
            )
        });
        let unsigned = upow(cmin)
            .and_then(|p| a.ulo.checked_mul(p))
            .zip(upow(cmax).and_then(|p| a.uhi.checked_mul(p)));
        Range::exact(signed, unsigned, width)
    }
}

/// The smallest and largest per-lane count of a constant shift amount, if every lane is a
/// constant below `width` (larger counts make the lane `poison`).
fn shift_counts(value: LLVMValueRef, width: u32) -> Option<(u32, u32)> {
    let counts = constant_values(value)?;
    let (lo, hi) = (*counts.iter().min()?, *counts.iter().max()?);
    (hi < u64::from(width)).then_some((lo as u32, hi as u32))
}

/// The smallest and largest per-lane divisor of a constant, if every lane is a non-zero constant.
fn divisors(value: LLVMValueRef) -> Option<(u128, u128)> {
    let ds = constant_values(value)?;
    let (lo, hi) = (*ds.iter().min()?, *ds.iter().max()?);
    (lo > 0).then_some((u128::from(lo), u128::from(hi)))
}

/// One range query: memoized results under the current loop-`phi` assumptions.
#[derive(Default)]
struct Ranges {
    /// Ranges assumed for `phi`s whose loop is being analysed (innermost last).
    assumed: Vec<(LLVMValueRef, Range)>,
    /// Results under `assumed` as it is now, keyed by value and remaining depth.
    cache: HashMap<(LLVMValueRef, u32), Range>,
}

impl Ranges {
    /// [`Range`] of an integer (or integer vector, over all lanes) from its definition, looking
    /// through at most `depth` instructions.
    fn of(&mut self, value: LLVMValueRef, depth: u32) -> Range {
        if let Some(&range) = self.cache.get(&(value, depth)) {
            return range;
        }
        let range = self.compute(value, depth);
        self.cache.insert((value, depth), range);
        range
    }

    fn compute(&mut self, value: LLVMValueRef, depth: u32) -> Range {
        let width = int_width(value);
        if width == 0 || width > MAX_WIDTH {
            // Never consulted for a check: `may_wrap` gives up on such widths.
            return Range::full(width.clamp(1, MAX_WIDTH));
        }
        let full = Range::full(width);
        if let Some(lanes) = constant_lanes(value) {
            return lanes.into_iter().fold(Range::EMPTY, |acc, lane| {
                acc.join(lane.map_or(full, |lane| constant_range(lane, width)))
            });
        }
        if depth == 0 || unsafe { LLVMIsAInstruction(value) }.is_null() {
            return full;
        }
        let operand_ref = |i| unsafe { LLVMGetOperand(value, i) };
        let opcode = unsafe { LLVMGetInstructionOpcode(value) };
        if opcode == LLVMOpcode::LLVMPHI {
            return self.phi(value, depth);
        }
        let operands = match opcode {
            LLVMOpcode::LLVMSelect => vec![operand_ref(1), operand_ref(2)],
            LLVMOpcode::LLVMZExt | LLVMOpcode::LLVMSExt | LLVMOpcode::LLVMTrunc => {
                vec![operand_ref(0)]
            }
            LLVMOpcode::LLVMCall if min_max(value).is_none() => return full,
            LLVMOpcode::LLVMCall
            | LLVMOpcode::LLVMAdd
            | LLVMOpcode::LLVMSub
            | LLVMOpcode::LLVMMul
            | LLVMOpcode::LLVMAnd
            | LLVMOpcode::LLVMOr
            | LLVMOpcode::LLVMXor
            | LLVMOpcode::LLVMShl
            | LLVMOpcode::LLVMLShr
            | LLVMOpcode::LLVMAShr
            | LLVMOpcode::LLVMUDiv
            | LLVMOpcode::LLVMURem => vec![operand_ref(0), operand_ref(1)],
            _ => return full,
        };
        let ranges: Vec<Range> = operands.iter().map(|&v| self.of(v, depth - 1)).collect();
        // Under the assumption of an empty `phi` (the start of its fixpoint), so is the result.
        if ranges.iter().any(|r| r.is_empty()) {
            return Range::EMPTY;
        }
        let a = ranges[0];
        let b = ranges.get(1).copied().unwrap_or(a);
        let unsigned_upto = |hi: u128| Range::from_unsigned(0, hi, width);
        let signed_bits = |bits: u32| {
            let bits = bits.clamp(1, width);
            Range::from_signed(smin_of(bits), smax_of(bits), width)
        };
        match opcode {
            LLVMOpcode::LLVMZExt => Range::from_unsigned(a.ulo, a.uhi, width),
            LLVMOpcode::LLVMSExt => Range::from_signed(a.slo, a.shi, width),
            LLVMOpcode::LLVMTrunc => {
                Range::exact(Some((a.slo, a.shi)), Some((a.ulo, a.uhi)), width).range(width)
            }
            LLVMOpcode::LLVMAdd => Range::add(a, b, width).range(width),
            LLVMOpcode::LLVMSub => Range::sub(a, b, width).range(width),
            LLVMOpcode::LLVMMul => Range::mul(a, b, width).range(width),
            // No bit is set that is clear in either operand; two sign extensions from `n` bits
            // combine to one.
            LLVMOpcode::LLVMAnd => unsigned_upto(a.uhi.min(b.uhi))
                .meet(signed_bits(a.signed_bits().max(b.signed_bits()))),
            // No bit above both operands' highest is set.
            LLVMOpcode::LLVMOr | LLVMOpcode::LLVMXor => {
                let bits = a.unsigned_bits().max(b.unsigned_bits());
                unsigned_upto(umax_of(bits.max(1)) >> u32::from(bits == 0))
                    .meet(signed_bits(a.signed_bits().max(b.signed_bits())))
            }
            LLVMOpcode::LLVMShl => match shift_counts(operand_ref(1), width) {
                Some(counts) => Range::shl(a, counts, width).range(width),
                None => full,
            },
            // Lanes shifted by at least the width are poison; the others move down by at least
            // the smallest count.
            LLVMOpcode::LLVMLShr => {
                match constant_values(operand_ref(1)).and_then(|counts| counts.into_iter().min()) {
                    Some(c) if c < u64::from(width) => unsigned_upto(a.uhi >> c),
                    _ => full,
                }
            }
            LLVMOpcode::LLVMAShr => match shift_counts(operand_ref(1), width) {
                Some((cmin, cmax)) => {
                    let values = [a.slo >> cmin, a.slo >> cmax, a.shi >> cmin, a.shi >> cmax];
                    let lo = values.iter().copied().min().unwrap_or(0);
                    let hi = values.iter().copied().max().unwrap_or(0);
                    Range::from_signed(lo, hi, width)
                }
                None => full,
            },
            // A zero divisor is immediate UB.
            LLVMOpcode::LLVMUDiv => match divisors(operand_ref(1)) {
                Some((dmin, dmax)) => Range::from_unsigned(a.ulo / dmax, a.uhi / dmin, width),
                None => unsigned_upto(a.uhi),
            },
            LLVMOpcode::LLVMURem => match divisors(operand_ref(1)) {
                Some((_, dmax)) => unsigned_upto(a.uhi.min(dmax - 1)),
                None => unsigned_upto(a.uhi),
            },
            LLVMOpcode::LLVMSelect => a.join(b),
            LLVMOpcode::LLVMCall => {
                // The result is one of the operands, and in the order the intrinsic compares by.
                let either = a.join(b);
                match min_max(value) {
                    Some(MinMax::SMin) => either.meet(Range::from_signed(
                        a.slo.min(b.slo),
                        a.shi.min(b.shi),
                        width,
                    )),
                    Some(MinMax::SMax) => either.meet(Range::from_signed(
                        a.slo.max(b.slo),
                        a.shi.max(b.shi),
                        width,
                    )),
                    Some(MinMax::UMin) => either.meet(Range::from_unsigned(
                        a.ulo.min(b.ulo),
                        a.uhi.min(b.uhi),
                        width,
                    )),
                    Some(MinMax::UMax) => either.meet(Range::from_unsigned(
                        a.ulo.max(b.ulo),
                        a.uhi.max(b.uhi),
                        width,
                    )),
                    None => full,
                }
            }
            _ => full,
        }
    }

    /// Range of a `phi`: the join of its incoming values. Around a loop an incoming value can
    /// depend on the `phi` itself, so assume a range for it and accept the assumption once the
    /// incoming values provably stay within it (by induction over the loop's iterations).
    fn phi(&mut self, value: LLVMValueRef, depth: u32) -> Range {
        if let Some(&(_, assumed)) = self.assumed.iter().rev().find(|(phi, _)| *phi == value) {
            return assumed;
        }
        let incoming = unsafe { LLVMCountIncoming(value) };
        let mut guess = Range::EMPTY;
        for _ in 0..PHI_ITERATIONS {
            // Cached results depend on the assumptions they were computed under.
            self.assumed.push((value, guess));
            let saved = std::mem::take(&mut self.cache);
            let next = (0..incoming).fold(Range::EMPTY, |acc, i| {
                acc.join(self.of(unsafe { LLVMGetIncomingValue(value, i) }, depth - 1))
            });
            self.cache = saved;
            self.assumed.pop();
            if next.within(guess) {
                return guess;
            }
            guess = guess.join(next);
        }
        Range::full(int_width(value))
    }
}

/// The exact range of a constant lane.
fn constant_range(lane: LLVMValueRef, width: u32) -> Range {
    let (zext, sext) = unsafe {
        (
            LLVMConstIntGetZExtValue(lane),
            LLVMConstIntGetSExtValue(lane),
        )
    };
    Range {
        slo: i128::from(sext),
        shi: i128::from(sext),
        ulo: u128::from(zext),
        uhi: u128::from(zext),
    }
    .meet(Range::full(width))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MinMax {
    SMin,
    SMax,
    UMin,
    UMax,
}

/// Which of `llvm.{s,u}{min,max}` the call `value` is, if any.
fn min_max(value: LLVMValueRef) -> Option<MinMax> {
    let callee = unsafe { LLVMGetCalledValue(value) };
    if callee.is_null() || unsafe { LLVMIsAFunction(callee) }.is_null() {
        return None;
    }
    let id = unsafe { LLVMGetIntrinsicID(callee) };
    if id == 0 {
        return None;
    }
    let lookup = |name: &str| unsafe { LLVMLookupIntrinsicID(name.as_ptr().cast(), name.len()) };
    [
        ("llvm.smin", MinMax::SMin),
        ("llvm.smax", MinMax::SMax),
        ("llvm.umin", MinMax::UMin),
        ("llvm.umax", MinMax::UMax),
    ]
    .into_iter()
    .find(|&(name, _)| lookup(name) == id)
    .map(|(_, kind)| kind)
}

/// Integer width of `value`'s type, or of its lanes for a vector; 0 if it is not an integer.
fn int_width(value: LLVMValueRef) -> u32 {
    unsafe {
        let mut typ = LLVMTypeOf(value);
        if LLVMGetTypeKind(typ) == LLVMTypeKind::LLVMVectorTypeKind {
            typ = LLVMGetElementType(typ);
        }
        match LLVMGetTypeKind(typ) {
            LLVMTypeKind::LLVMIntegerTypeKind => LLVMGetIntTypeWidth(typ),
            _ => 0,
        }
    }
}

/// The lanes of a constant integer or integer vector, each `None` unless it is a `ConstantInt` of
/// at most 64 bits (e.g. `poison`); `None` if `value` is not such a constant.
fn constant_lanes(value: LLVMValueRef) -> Option<Vec<Option<LLVMValueRef>>> {
    unsafe {
        let lanes = if !LLVMIsAConstantInt(value).is_null() {
            vec![value]
        } else if !LLVMIsAConstantAggregateZero(value).is_null()
            && LLVMGetTypeKind(LLVMTypeOf(value)) == LLVMTypeKind::LLVMVectorTypeKind
        {
            // `zeroinitializer`: every lane is the zero of the element type.
            let lane = LLVMConstNull(LLVMGetElementType(LLVMTypeOf(value)));
            vec![lane; LLVMGetVectorSize(LLVMTypeOf(value)) as usize]
        } else if !LLVMIsAConstantDataVector(value).is_null()
            || !LLVMIsAConstantVector(value).is_null()
        {
            let count = LLVMGetVectorSize(LLVMTypeOf(value));
            (0..count)
                .map(|i| LLVMGetAggregateElement(value, i))
                .collect()
        } else {
            return None;
        };
        Some(
            lanes
                .into_iter()
                .map(|lane| {
                    (!lane.is_null()
                        && !LLVMIsAConstantInt(lane).is_null()
                        && int_width(lane) <= 64)
                        .then_some(lane)
                })
                .collect(),
        )
    }
}

/// The lane values of a constant integer (vector), if every lane is a known constant.
fn constant_values(value: LLVMValueRef) -> Option<Vec<u64>> {
    constant_lanes(value)?
        .into_iter()
        .map(|lane| lane.map(|lane| unsafe { LLVMConstIntGetZExtValue(lane) }))
        .collect()
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;

    use super::{
        blocks_in_rpo, loop_contiguous_order, lowering_order, may_wrap, sole_user_is_next,
    };

    /// `may_wrap` (signed, unsigned) for each named instruction of `@f` in `ir`.
    fn wraps(ir: &str) -> Vec<(String, bool, bool)> {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir.as_bytes(), "test");
        let module = context.create_module_from_ir(buffer).expect("valid IR");
        let function = module.get_function("f").expect("@f");
        function
            .get_basic_blocks()
            .into_iter()
            .flat_map(|block| block.get_instructions())
            .filter(|i| {
                i.get_name()
                    .is_some_and(|n| n.to_bytes().starts_with(b"w."))
            })
            .map(|i| {
                let name = i.get_name().unwrap().to_string_lossy().into_owned();
                let (signed, unsigned) = may_wrap(i);
                (name, signed, unsigned)
            })
            .collect()
    }

    fn check(body: &str, params: &str, expected: &[(&str, bool, bool)]) {
        let decls: String = ["smin", "smax", "umin", "umax"]
            .iter()
            .map(|m| format!("declare i32 @llvm.{m}.i32(i32, i32)\n"))
            .collect();
        let ir = format!("{decls}define void @f({params}) {{\n{body}\n  ret void\n}}\n");
        let expected: Vec<_> = expected
            .iter()
            .map(|&(n, s, u)| (n.to_owned(), s, u))
            .collect();
        assert_eq!(wraps(&ir), expected);
    }

    #[test]
    fn blocks_in_rpo_then_loops_contiguous() {
        // Textual order puts the exit before the loop body; `dead` is unreachable.
        let ir = "define void @f(i1 %c) {\nentry:\n  br label %header\nexit:\n  ret void\n\
                  dead:\n  br label %exit\nheader:\n  br i1 %c, label %body, label %exit\n\
                  body:\n  br label %header\n}\n";
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir.as_bytes(), "test");
        let module = context.create_module_from_ir(buffer).expect("valid IR");
        let function = module.get_function("f").unwrap();
        let names = |blocks: Vec<inkwell::basic_block::BasicBlock<'_>>| {
            blocks
                .iter()
                .map(|block| block.get_name().to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(blocks_in_rpo(function)),
            ["entry", "header", "exit", "body"]
        );
        assert_eq!(
            names(lowering_order(function)),
            ["entry", "header", "body", "exit"]
        );
    }

    #[test]
    fn sole_user_is_next_instruction() {
        let ir = "define i1 @f(i32 %x) {\n  %next = icmp eq i32 %x, 0\n  %a = xor i1 %next, true\n  \
                  %later = icmp eq i32 %x, 1\n  %skip = icmp eq i32 %x, 2\n  \
                  %b = and i1 %later, %skip\n  %twice = icmp eq i32 %x, 3\n  \
                  %c = and i1 %twice, %twice\n  %d = or i1 %a, %b\n  %e = or i1 %d, %c\n  \
                  ret i1 %e\n}\n";
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir.as_bytes(), "test");
        let module = context.create_module_from_ir(buffer).expect("valid IR");
        let block = module
            .get_function("f")
            .unwrap()
            .get_first_basic_block()
            .unwrap();
        let named = |name: &str| block.get_instruction_with_name(name).unwrap();
        assert_eq!(sole_user_is_next(named("next")), Some(named("a")));
        assert_eq!(sole_user_is_next(named("skip")), Some(named("b")));
        // Used, but not by the next instruction; or used twice by it.
        assert_eq!(sole_user_is_next(named("later")), None);
        assert_eq!(sole_user_is_next(named("twice")), None);
    }

    #[test]
    fn promoted_narrow_values_cannot_wrap() {
        check(
            "  %a = zext i8 %x to i32\n  %b = zext i8 %y to i32\n  %w.sub = sub i32 %a, %b\n  \
             %w.mul = mul i32 %a, %b\n  %w64 = zext i8 %x to i64\n  %w.neg = sub i64 0, %w64",
            "i8 %x, i8 %y",
            // Unsigned `sub` can still wrap (a < b); so can `0 - zext(i8)`, a negated byte, in i64.
            &[
                ("w.sub", false, true),
                ("w.mul", false, false),
                ("w.neg", false, true),
            ],
        );
    }

    #[test]
    fn bounds_follow_non_wrapping_arithmetic() {
        check(
            "  %a = zext i8 %x to i32\n  %s = add i32 %a, %a\n  %w.chain = sub i32 %s, %a\n  \
             %c = select i1 %b, i32 %a, i32 255\n  %w.sel = mul i32 %c, %c\n  \
             %m = mul i32 %a, %a\n  %mm = mul i32 %m, %m\n  %w.wide = add i32 %mm, 1",
            "i8 %x, i1 %b",
            // `(a + a) - a` is in [-255, 510]; (a*a)*(a*a) <= 255^4 fits u32 (not i32), so
            // adding 1 cannot wrap unsigned.
            &[
                ("w.chain", false, true),
                ("w.sel", false, false),
                ("w.wide", true, false),
            ],
        );
    }

    #[test]
    fn products_need_the_sum_of_operand_bits() {
        check(
            "  %a = zext i16 %x to i32\n  %w.mul = mul i32 %a, %a",
            "i16 %x",
            // 65535 * 65535 overflows `i32` as signed but not as unsigned.
            &[("w.mul", true, false)],
        );
    }

    #[test]
    fn masks_and_shifts_bound_values() {
        check(
            "  %h = lshr i32 %p, 24\n  %l = and i32 %p, 255\n  %w.add = add i32 %h, %l\n  \
             %w.shl = shl i32 %l, 24\n  %w.var = shl i32 %l, %p",
            "i32 %p",
            // 255 << 24 sets the sign bit; a variable shift is never bounded.
            &[
                ("w.add", false, false),
                ("w.shl", true, false),
                ("w.var", true, true),
            ],
        );
    }

    #[test]
    fn vector_lshr_bounds_use_the_smallest_shift() {
        check(
            "  %h = lshr <2 x i32> %p, <i32 24, i32 1>\n  %w.add = add <2 x i32> %h, %h",
            "<2 x i32> %p",
            // The lane shifted by 1 keeps 31 bits: doubling it may wrap as signed, not as unsigned.
            &[("w.add", true, false)],
        );
    }

    #[test]
    fn sign_extension_and_constants() {
        check(
            "  %s = sext i8 %x to i32\n  %w.add = add i32 %s, %s\n  %w.min = add i8 %x, -128\n  \
             %w.unk = add i32 %p, 1",
            "i8 %x, i32 %p",
            &[
                ("w.add", false, true),
                ("w.min", true, true),
                ("w.unk", true, true),
            ],
        );
    }

    #[test]
    fn vector_lanes_are_bounded_together() {
        check(
            "  %v = zext <4 x i8> %x to <4 x i16>\n  %w.add = add <4 x i16> %v, splat (i16 1)\n  \
             %w.mix = add <2 x i16> <i16 1, i16 32767>, <i16 1, i16 1>",
            "<4 x i8> %x",
            &[("w.add", false, false), ("w.mix", true, false)],
        );
    }

    #[test]
    fn or_and_shl_bound_packed_values() {
        check(
            "  %u = zext i8 %a to i32\n  %v = zext i8 %b to i32\n  %s = shl i32 %v, 16\n  \
             %p = or disjoint i32 %s, %u\n  %w.mul = mul i32 %p, 3\n  %w.big = mul i32 %p, 512\n  \
             %l = and i32 %q, 255\n  %h = shl i32 %l, 8\n  %w.add = add i32 %h, %h",
            "i8 %a, i8 %b, i32 %q",
            // A packed `u | v << 16` of two bytes (below 2^24) times 3 cannot wrap;
            // times 512 needs 33 bits.
            &[
                ("w.mul", false, false),
                ("w.big", true, true),
                ("w.add", false, false),
            ],
        );
    }

    #[test]
    fn xor_keeps_sign_extension() {
        check(
            "  %s = sext i8 %a to i32\n  %t = sext i8 %b to i32\n  %x = xor i32 %s, %t\n  \
             %w.add = add i32 %x, %x",
            "i8 %a, i8 %b",
            // 8 signed bits, but possibly negative.
            &[("w.add", false, true)],
        );
    }

    #[test]
    fn arithmetic_shift_right_keeps_the_sign() {
        check(
            "  %h = ashr i32 %p, 24\n  %w.neg = add i32 %h, %h\n  %z = zext i16 %b to i32\n  \
             %q = ashr i32 %z, 8\n  %w.pos = mul i32 %q, %q",
            "i32 %p, i16 %b",
            &[("w.neg", false, true), ("w.pos", false, false)],
        );
    }

    #[test]
    fn quotients_and_remainders() {
        check(
            "  %d = udiv i32 %p, 65536\n  %w.div = mul i32 %d, %d\n  %r = urem i32 %p, 100\n  \
             %w.rem = mul i32 %r, %r\n  %v = udiv i32 %p, %p\n  %w.var = add i32 %v, 1",
            "i32 %p",
            // 16 bits squared fits unsigned only; a variable divisor proves nothing about `%p`.
            &[
                ("w.div", true, false),
                ("w.rem", false, false),
                ("w.var", true, true),
            ],
        );
    }

    #[test]
    fn clamps_bound_values() {
        check(
            "  %lo = call i32 @llvm.smin.i32(i32 %p, i32 255)\n  \
             %c = call i32 @llvm.smax.i32(i32 %lo, i32 0)\n  %w.clamp = mul i32 %c, %c\n  \
             %m = call i32 @llvm.smax.i32(i32 %p, i32 0)\n  %w.half = add i32 %m, %m\n  \
             %u = call i32 @llvm.umin.i32(i32 %p, i32 255)\n  %w.umin = mul i32 %u, %u\n  \
             %x = call i32 @llvm.umax.i32(i32 %p, i32 255)\n  %w.umax = add i32 %x, 1",
            "i32 %p",
            // A clamp to [0, 255]; `smax(p, 0)` is non-negative but may use 31 bits.
            &[
                ("w.clamp", false, false),
                ("w.half", true, false),
                ("w.umin", false, false),
                ("w.umax", true, true),
            ],
        );
    }

    #[test]
    fn loop_phi_that_does_not_grow_is_bounded() {
        check(
            "  %z0 = zext i8 %a to i32\n  br label %loop\nloop:\n  \
             %acc = phi i32 [ %z0, %0 ], [ %next, %loop ]\n  %w.mul = mul i32 %acc, 3\n  \
             %b = load i8, ptr %src\n  %next = zext i8 %b to i32\n  \
             %c = icmp eq i32 %w.mul, 0\n  br i1 %c, label %exit, label %loop\nexit:",
            "i8 %a, ptr %src",
            &[("w.mul", false, false)],
        );
    }

    #[test]
    fn loop_phi_that_grows_is_unbounded() {
        check(
            "  %z = zext i8 %a to i32\n  br label %loop\nloop:\n  \
             %acc = phi i32 [ 0, %0 ], [ %w.next, %loop ]\n  %w.next = add i32 %acc, %z\n  \
             %c = icmp eq i32 %w.next, 0\n  br i1 %c, label %exit, label %loop\nexit:",
            "i8 %a",
            // Each iteration adds up to 255: no bound survives induction.
            &[("w.next", true, true)],
        );
    }

    #[test]
    fn join_phi_without_a_loop() {
        check(
            "  br i1 %c, label %l, label %r\nl:\n  %x = zext i8 %a to i32\n  br label %j\nr:\n  \
             br label %j\nj:\n  %p = phi i32 [ %x, %l ], [ 1000, %r ]\n  %w.mul = mul i32 %p, %p",
            "i1 %c, i8 %a",
            &[("w.mul", false, false)],
        );
    }

    #[test]
    fn long_accumulations_are_bounded() {
        // 8 + sixteen bytes, unrolled; deeper than a fixed small recursion limit.
        let mut body = String::from("  %s0 = add i32 0, 8\n");
        for i in 1..=16 {
            body += &format!(
                "  %z{i} = zext i8 %a to i32\n  %{n}{i} = add i32 %s{p}, %z{i}\n",
                n = if i == 16 { "w.s" } else { "s" },
                p = i - 1
            );
        }
        check(&body, "i8 %a", &[("w.s16", false, false)]);
    }

    #[test]
    fn narrowed_sums_use_magnitudes_not_bit_counts() {
        // The same sum after LLVM narrows it to i16: 8 + sixteen bytes <= 4088.
        let mut body = String::from("  %s0 = add i16 0, 8\n");
        for i in 1..=16 {
            body += &format!(
                "  %z{i} = zext i8 %a to i16\n  %{n}{i} = add i16 %s{p}, %z{i}\n",
                n = if i == 16 { "w.s" } else { "s" },
                p = i - 1
            );
        }
        body += "  %y = zext i8 %a to i16\n  %w.mul = mul i16 %y, 129";
        // 255 * 129 = 32895 fits u16 but not i16.
        check(
            &body,
            "i8 %a",
            &[("w.s16", false, false), ("w.mul", true, false)],
        );
    }

    #[test]
    fn soundness_edges() {
        check(
            "  %z = zext <2 x i8> %v to <2 x i32>\n  %n = sub <2 x i32> zeroinitializer, %z\n  \
             %w.shl = shl <2 x i32> %n, <i32 1, i32 24>\n  \
             %x = sext i8 %a to i32\n  %m = call i32 @llvm.smin.i32(i32 %x, i32 100)\n  \
             %w.smin = sub i32 %m, 2147483600\n  \
             %r = urem <2 x i8> %u, <i8 7, i8 100>\n  %w.urem = mul <2 x i8> %r, splat (i8 3)\n  \
             %y = zext i8 %a to i32\n  %s = select i1 %c, i32 %y, i32 %p\n  %w.sel = add i32 %s, 1\n  \
             %d = udiv <2 x i8> %u, <i8 1, i8 100>\n  %w.udiv = mul <2 x i8> %d, splat (i8 2)",
            "<2 x i8> %v, i8 %a, <2 x i8> %u, i1 %c, i32 %p",
            // -255 << 24 is below i32's minimum; smin keeps x's lower bound; the remainder by the
            // larger divisor reaches 99; a select may pick its unbounded operand; the quotient by
            // the smaller divisor reaches 255.
            &[
                ("w.shl", true, true),
                ("w.smin", true, true),
                ("w.urem", true, true),
                ("w.sel", true, true),
                ("w.udiv", true, true),
            ],
        );
    }

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
