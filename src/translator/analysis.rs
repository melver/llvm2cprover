// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Facts about the input LLVM IR that lowering consults.
//!
//! Analyses are queries: they do not rewrite IR and know nothing about GOTO. Each one only ever
//! proves a property (e.g. that an operation cannot wrap); an inconclusive answer must leave the
//! lowering as it would be without the analysis, so a weak analysis costs performance, not
//! soundness.

use std::collections::{BTreeMap, BTreeSet};

use inkwell::llvm_sys::LLVMOpcode;
use inkwell::llvm_sys::LLVMTypeKind;
use inkwell::llvm_sys::core::{
    LLVMConstIntGetSExtValue, LLVMConstIntGetZExtValue, LLVMGetAggregateElement,
    LLVMGetElementType, LLVMGetInstructionOpcode, LLVMGetIntTypeWidth, LLVMGetOperand,
    LLVMGetTypeKind, LLVMGetVectorSize, LLVMIsAConstantDataVector, LLVMIsAConstantInt,
    LLVMIsAConstantVector, LLVMIsAInstruction, LLVMTypeOf,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::values::{AsValueRef, InstructionValue};

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

/// Whether `instruction` (`add`/`sub`/`mul`/`shl`) can wrap in the signed (`nsw`) or unsigned
/// (`nuw`) interpretation, i.e. whether that flag can make its result `poison`. `false` only when
/// operand bounds prove it cannot, as for C arithmetic on integer-promoted narrow values.
pub(super) fn may_wrap(instruction: InstructionValue<'_>, signed: bool) -> bool {
    let value = instruction.as_value_ref();
    let (lhs, rhs) = unsafe { (LLVMGetOperand(value, 0), LLVMGetOperand(value, 1)) };
    let width = int_width(value);
    let bits = |v| {
        let bounds = bit_bounds(v, MAX_DEPTH);
        if signed {
            bounds.signed
        } else {
            bounds.unsigned
        }
    };
    match unsafe { LLVMGetInstructionOpcode(value) } {
        LLVMOpcode::LLVMAdd => bits(lhs).max(bits(rhs)) + 1 > width,
        // Unsigned subtraction wraps unless the subtrahend is provably zero.
        LLVMOpcode::LLVMSub if signed => bits(lhs).max(bits(rhs)) + 1 > width,
        LLVMOpcode::LLVMSub => bits(rhs) > 0,
        LLVMOpcode::LLVMMul => bits(lhs) + bits(rhs) > width,
        LLVMOpcode::LLVMShl => {
            // The largest per-lane shift grows the value the most.
            let shift = constant_values(rhs).and_then(|shifts| shifts.into_iter().max());
            shift.is_none_or(|shift| u64::from(bits(lhs)) + shift > u64::from(width))
        }
        _ => true,
    }
}

/// Bits needed to represent a value: `signed` in two's complement, `unsigned` as a non-negative
/// number. Both are at most the value's width, which is what nothing is known about it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BitBounds {
    signed: u32,
    unsigned: u32,
}

const MAX_DEPTH: u32 = 8;

/// [`BitBounds`] of an integer (or integer vector, over all lanes) from its definition: constants,
/// `zext`, `sext`, `trunc`, `and` and `lshr` by a constant, looking through at most `depth`
/// instructions.
fn bit_bounds(value: LLVMValueRef, depth: u32) -> BitBounds {
    let width = int_width(value);
    let unknown = BitBounds {
        signed: width,
        unsigned: width,
    };
    // A non-negative value of `unsigned` bits needs one more as a signed number.
    let from_unsigned = |unsigned: u32| BitBounds {
        signed: (unsigned + 1).min(width),
        unsigned,
    };
    if width == 0 {
        return unknown;
    }
    if let Some(lanes) = constant_lanes(value) {
        return lanes.into_iter().fold(
            BitBounds {
                signed: 0,
                unsigned: 0,
            },
            |acc, lane| {
                let lane = lane.map_or(unknown, constant_bounds);
                BitBounds {
                    signed: acc.signed.max(lane.signed),
                    unsigned: acc.unsigned.max(lane.unsigned),
                }
            },
        );
    }
    if depth == 0 || unsafe { LLVMIsAInstruction(value) }.is_null() {
        return unknown;
    }
    let operand = |i| bit_bounds(unsafe { LLVMGetOperand(value, i) }, depth - 1);
    match unsafe { LLVMGetInstructionOpcode(value) } {
        LLVMOpcode::LLVMZExt => from_unsigned(operand(0).unsigned),
        LLVMOpcode::LLVMSExt => {
            let source = operand(0);
            let source_width = int_width(unsafe { LLVMGetOperand(value, 0) });
            // Non-negative sources stay non-negative; others fill the new high bits with ones.
            let unsigned = if source.unsigned < source_width {
                source.unsigned
            } else {
                width
            };
            BitBounds {
                signed: source.signed,
                unsigned,
            }
        }
        LLVMOpcode::LLVMTrunc => {
            let source = operand(0);
            BitBounds {
                signed: if source.signed <= width {
                    source.signed
                } else {
                    width
                },
                unsigned: source.unsigned.min(width),
            }
        }
        LLVMOpcode::LLVMAnd => from_unsigned(operand(0).unsigned.min(operand(1).unsigned)),
        // Exact results of narrow operands, kept only if they fit `width` (so cannot have
        // wrapped); a `sub` may go negative, so its unsigned bound is lost.
        LLVMOpcode::LLVMAdd | LLVMOpcode::LLVMSub | LLVMOpcode::LLVMMul => {
            let (a, b) = (operand(0), operand(1));
            let (signed, unsigned) = match unsafe { LLVMGetInstructionOpcode(value) } {
                LLVMOpcode::LLVMAdd => (a.signed.max(b.signed) + 1, a.unsigned.max(b.unsigned) + 1),
                LLVMOpcode::LLVMSub => (a.signed.max(b.signed) + 1, width),
                _ => (a.signed + b.signed, a.unsigned + b.unsigned),
            };
            BitBounds {
                signed: signed.min(width),
                unsigned: unsigned.min(width),
            }
        }
        LLVMOpcode::LLVMSelect => {
            let (a, b) = (operand(1), operand(2));
            BitBounds {
                signed: a.signed.max(b.signed),
                unsigned: a.unsigned.max(b.unsigned),
            }
        }
        // The smallest per-lane shift leaves the most bits. Lanes shifted by at least the width
        // are poison, which nothing downstream can make worse.
        LLVMOpcode::LLVMLShr => match constant_values(unsafe { LLVMGetOperand(value, 1) })
            .and_then(|shifts| shifts.into_iter().min())
        {
            Some(shift) if shift < u64::from(width) => {
                from_unsigned(operand(0).unsigned.saturating_sub(shift as u32))
            }
            _ => unknown,
        },
        _ => unknown,
    }
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

fn constant_bounds(lane: LLVMValueRef) -> BitBounds {
    let (zext, sext) = unsafe {
        (
            LLVMConstIntGetZExtValue(lane),
            LLVMConstIntGetSExtValue(lane),
        )
    };
    // A negative value `v` needs as many bits as `!v`, plus the sign bit.
    let magnitude = if sext < 0 { !sext } else { sext };
    BitBounds {
        signed: 64 - magnitude.leading_zeros() + 1,
        unsigned: 64 - zext.leading_zeros(),
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

    use super::{loop_contiguous_order, may_wrap};

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
                (name, may_wrap(i, true), may_wrap(i, false))
            })
            .collect()
    }

    fn check(body: &str, params: &str, expected: &[(&str, bool, bool)]) {
        let ir = format!("define void @f({params}) {{\n{body}\n  ret void\n}}\n");
        let expected: Vec<_> = expected
            .iter()
            .map(|&(n, s, u)| (n.to_owned(), s, u))
            .collect();
        assert_eq!(wraps(&ir), expected);
    }

    #[test]
    fn promoted_narrow_values_cannot_wrap() {
        check(
            "  %a = zext i8 %x to i32\n  %b = zext i8 %y to i32\n  %w.sub = sub i32 %a, %b\n  \
             %w.mul = mul i32 %a, %b\n  %w64 = zext i8 %x to i64\n  %w.neg = sub i64 0, %w64",
            "i8 %x, i8 %y",
            // Unsigned `sub` can still wrap (a < b); `0 - zext(i8)` in i64 is TrueMotion's case.
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
            // `(a + a) - a` needs 10 signed bits; (a*a)*(a*a) may wrap, so its bound is lost.
            &[
                ("w.chain", false, true),
                ("w.sel", false, false),
                ("w.wide", true, true),
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
