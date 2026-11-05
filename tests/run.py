#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The llvm2cprover Authors

"""Run end-to-end translator tests and print TAP."""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
import tomllib


ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "tests"
SOURCE_EXTS = {".c", ".cc", ".cpp", ".ll", ".rs"}
EXPECTED_RESULTS = {
    "VERIFICATION SUCCESSFUL",
    "VERIFICATION FAILED",
    "TRANSLATION FAILED",
}


@dataclass(frozen=True)
class TestCase:
    path: Path
    category: str
    name: str
    description: str
    expected: str
    contracts: tuple[Path, ...] = ()
    entry: str | None = None
    clang_flags: tuple[str, ...] = ()
    rustc_flags: tuple[str, ...] = ()
    cbmc_flags: tuple[str, ...] = ()
    translator_flags: tuple[str, ...] = ()
    skip: bool = False
    skip_reason: str = ""
    esbmc_skip: str = ""
    todo: str = ""

    @property
    def label(self) -> str:
        return f"{self.category}/{self.name}"


@dataclass(frozen=True)
class RunResult:
    ok: bool
    details: tuple[str, ...] = field(default_factory=tuple)
    skipped: bool = False


def discover_tests(pattern: str) -> list[TestCase]:
    tests = []
    for path in sorted(TESTS.rglob("*")):
        if path.name == "run.py" or path.suffix not in SOURCE_EXTS:
            continue
        if path.parent == TESTS:
            continue
        case = load_test(path)
        if fnmatch.fnmatch(case.label, pattern):
            tests.append(case)
    return tests


def load_test(path: Path) -> TestCase:
    comment = comment_prefix(path)
    metadata = extract_metadata(path.read_text(), comment)
    expected = metadata["expected"]
    if expected not in EXPECTED_RESULTS:
        raise ValueError(f"{path}: unsupported expected result {expected!r}")
    return TestCase(
        path=path,
        category=path.parent.relative_to(TESTS).as_posix(),
        name=path.stem,
        description=metadata.get("description", ""),
        expected=expected,
        contracts=tuple(contract_manifests(path)),
        entry=metadata.get("entry"),
        clang_flags=tuple(metadata.get("clang_flags", [])),
        rustc_flags=tuple(metadata.get("rustc_flags", [])),
        cbmc_flags=tuple(metadata.get("cbmc_flags", [])),
        translator_flags=tuple(metadata.get("translator_flags", [])),
        skip=bool(metadata.get("skip", False)),
        skip_reason=metadata.get("skip_reason", ""),
        esbmc_skip=metadata.get("esbmc_skip", ""),
        todo=str(metadata.get("todo", "")),
    )


def comment_prefix(path: Path) -> str:
    if path.suffix in {".c", ".cc", ".cpp", ".rs"}:
        return "//"
    if path.suffix == ".ll":
        return ";"
    raise ValueError(f"{path}: unsupported test extension")


def extract_metadata(text: str, comment: str) -> dict[str, object]:
    marker = f"{comment} ---"
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines)
                 if line.strip() == marker), None)
    if start is None:
        raise ValueError("missing metadata start marker")
    end = next(
        (i for i in range(start + 1, len(lines))
         if lines[i].strip() == marker),
        None,
    )
    if end is None:
        raise ValueError("missing metadata end marker")
    block = []
    for line in lines[start + 1: end]:
        stripped = line.strip()
        if not stripped.startswith(comment):
            raise ValueError("metadata lines must use the test comment prefix")
        block.append(stripped.removeprefix(comment).lstrip())
    return tomllib.loads("\n".join(block))


def contract_manifests(path: Path) -> list[Path]:
    manifest = path.with_suffix(".toml")
    return [manifest] if manifest.is_file() else []


def contract_args(test: TestCase) -> list[str]:
    return [arg for manifest in test.contracts for arg in ("--contracts", str(manifest))]


def compile_command(test: TestCase, args: argparse.Namespace) -> tuple[str, str, list[str]] | None:
    if test.path.suffix == ".ll":
        return None
    if test.path.suffix == ".rs":
        # Rust E2E inputs compile as libraries with `panic=abort` so each test can expose a small
        # explicit harness and stay within the current no-unwinding translation boundary.
        return (
            "rustc",
            "ll",
            [
                args.rustc,
                "--crate-type=lib",
                "--emit=llvm-ir",
                "-C",
                "panic=abort",
                *test.rustc_flags,
                "-o",
                "-",
                str(test.path),
            ],
        )
    return (
        "clang",
        "bc",
        [
            args.clang,
            "-c",
            "-emit-llvm",
            "-O0",
            "-Xclang",
            "-disable-O0-optnone",
            *default_clang_flags(test.path),
            *test.clang_flags,
            "-o",
            "-",
            str(test.path),
        ],
    )


def default_clang_flags(path: Path) -> tuple[str, ...]:
    if path.suffix in {".cc", ".cpp"}:
        return ("-std=c++20",)
    return ()


def translate_module(
    llvm_module: subprocess.CompletedProcess[bytes],
    input_format: str,
    args: argparse.Namespace,
    goto_path: Path,
    contract_flags: list[str],
    translator_flags: tuple[str, ...],
) -> tuple[str, subprocess.CompletedProcess[str]]:
    translate = decode_result(
        subprocess.run(
            [
                args.llvm2cprover,
                "-",
                "--input-format",
                input_format,
                *contract_flags,
                *translator_flags,
                "-o",
                str(goto_path),
            ],
            cwd=ROOT,
            input=llvm_module.stdout,
            capture_output=True,
            check=False,
        )
    )
    if translate.returncode == 0:
        return ("llvm2cprover", translate)
    return (
        "llvm2cprover",
        subprocess.CompletedProcess(
            translate.args,
            translate.returncode,
            translate.stdout,
            merge_outputs(decode_stream(llvm_module.stderr), translate.stderr),
        ),
    )


def translate_test(
    test: TestCase,
    args: argparse.Namespace,
    goto_path: Path,
    mode_flags: tuple[str, ...] = (),
) -> tuple[str, subprocess.CompletedProcess[str]]:
    contract_flags = contract_args(test)
    translator_flags = (*test.translator_flags, *mode_flags)
    if test.path.suffix == ".ll":
        return (
            "llvm2cprover",
            run_command(
                [
                    args.llvm2cprover,
                    str(test.path),
                    "--input-format",
                    "ll",
                    *contract_flags,
                    *translator_flags,
                    "-o",
                    str(goto_path),
                ],
                cwd=ROOT,
            ),
        )
    compile_spec = compile_command(test, args)
    assert compile_spec is not None
    tool, input_format, command = compile_spec
    module = run_binary_command(command, cwd=ROOT)
    if module.returncode != 0:
        return (tool, decode_result(module))
    return translate_module(
        module,
        input_format,
        args,
        goto_path,
        contract_flags,
        translator_flags,
    )


def run_cbmc(
    test: TestCase, args: argparse.Namespace, goto_path: Path, extra: tuple[str, ...]
) -> subprocess.CompletedProcess[str]:
    command = [args.cbmc, str(goto_path)]
    if test.entry:
        command.extend(["--function", test.entry])
    command.extend(test.cbmc_flags)
    command.extend(extra)
    return run_command(command, cwd=ROOT)


def run_esbmc(test: TestCase, args: argparse.Namespace, goto_path: Path) -> subprocess.CompletedProcess[str]:
    # Without a CBMC-generated `__CPROVER__start`, ESBMC needs the entry point spelled out.
    command = [args.esbmc_bin, "--binary", str(goto_path), "--function", test.entry or "main"]
    command.extend(esbmc_flags(test.cbmc_flags))
    return run_command(command, cwd=ROOT)


def esbmc_flags(cbmc_flags: tuple[str, ...]) -> list[str]:
    """Map CBMC options to ESBMC's. CBMC's standard checks are ESBMC's defaults plus these."""
    standard = ["--overflow-check", "--ub-shift-check"]
    flags = []
    remaining = iter(cbmc_flags)
    for flag in remaining:
        if flag == "--unwind":
            flags += [flag, next(remaining)]
        elif flag == "--no-standard-checks":
            standard = ["--no-bounds-check", "--no-pointer-check", "--no-div-by-zero-check"]
        elif flag == "--pointer-check":
            standard.remove("--no-pointer-check")
        elif flag == "--memory-leak-check":
            flags.append(flag)
        elif flag != "--unwinding-assertions":  # ESBMC's default, as CBMC's
            raise ValueError(f"no ESBMC equivalent for CBMC option {flag}")
    return standard + flags


# Classes of the checks CBMC's goto_check generates. Which duplicates it emits depends on where its
# deduplication restarts, and CBMC's goto-convert splits `c ? nondet : v` into control flow that
# restarts it, so these are compared by presence. Other properties come from the translator's own
# assertions, which goto_check leaves alone, and are counted exactly.
GOTO_CHECK_CLASSES = {
    "array_bounds", "bit_count", "division-by-zero", "enum-range-check",
    "float-division-by-zero", "memory-leak", "NaN", "overflow", "pointer",
    "pointer_arithmetic", "pointer_dereference", "pointer_primitives", "undefined-shift",
}


def property_results(output: str) -> Counter[tuple[str, str, str]]:
    # Property names carry per-function counters, which shift with any duplicate, so compare by
    # name stem (function and class), location and description, and verdict.
    results: Counter[tuple[str, str, str]] = Counter()
    for name, what, status in re.findall(
        r"^\[(\S+)\] (.*): (SUCCESS|FAILURE|UNKNOWN)$", output, re.MULTILINE
    ):
        stem = re.sub(r"\.\d+$", "", name)
        key = (stem, what, status)
        if stem.rsplit(".", 1)[-1] in GOTO_CHECK_CLASSES:
            results[key] = 1
        else:
            results[key] += 1
    return results


def selected_enforcement(test: TestCase) -> str | None:
    selections = [
        test.translator_flags[index + 1]
        for index, flag in enumerate(test.translator_flags[:-1])
        if flag == "--enforce-contract"
    ]
    if not selections:
        return None
    if len(selections) != 1:
        raise ValueError(f"{test.path}: E2E discharge tests select exactly one contract")
    return selections[0]


def instrument_contract(
    test: TestCase,
    args: argparse.Namespace,
    goto_path: Path,
    target: str,
) -> tuple[str, subprocess.CompletedProcess[str], Path]:
    # Mirrors the symbol names documented in README.md, which llvm2cprover builds with
    # util::sanitize_name. Test targets are plain identifiers, so alnum-or-underscore is
    # enough here; a rename on the Rust side surfaces as a missing-property failure.
    safe_target = "".join(character if character.isalnum() else "_" for character in target)
    harness = f"__llvm2cprover_contract_enforcement_{safe_target}"
    specification = f"__llvm2cprover_contract_spec_{safe_target}"
    linked_path = goto_path.with_suffix(".linked.goto")
    instrumented_path = goto_path.with_suffix(".instrumented.goto")
    linked = run_command(
        [args.goto_cc, str(goto_path), "--function", harness, "-o", str(linked_path)],
        cwd=ROOT,
    )
    if linked.returncode != 0:
        return ("goto-cc", linked, linked_path)
    instrumented = run_command(
        [
            args.goto_instrument,
            "--dfcc",
            harness,
            "--no-malloc-may-fail",
            "--enforce-contract",
            f"{target}/{specification}",
            str(linked_path),
            str(instrumented_path),
        ],
        cwd=ROOT,
    )
    return ("goto-instrument", instrumented, instrumented_path)


def expect_translation_failure(result: subprocess.CompletedProcess[str]) -> RunResult:
    # Fail-closed means a diagnostic, not a crash: reject Rust panics (101) and signals (< 0).
    if result.returncode > 0 and result.returncode != 101:
        return RunResult(True)
    return RunResult(
        False,
        command_details("llvm2cprover", result),
    )


def run_command(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, capture_output=True, text=True, check=False)


def run_binary_command(command: list[str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(command, cwd=cwd, capture_output=True, check=False)


def decode_stream(data: bytes | str) -> str:
    if isinstance(data, bytes):
        return data.decode(errors="replace")
    return data


def decode_result(
    result: subprocess.CompletedProcess[bytes] | subprocess.CompletedProcess[str],
) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(
        result.args,
        result.returncode,
        decode_stream(result.stdout),
        decode_stream(result.stderr),
    )


def command_details(name: str, result: subprocess.CompletedProcess[str]) -> tuple[str, ...]:
    details = [f"{name} exit code: {result.returncode}"]
    if result.stdout:
        details.extend(trim_output("stdout", result.stdout))
    if result.stderr:
        details.extend(trim_output("stderr", result.stderr))
    return tuple(details)


def merge_outputs(*parts: str) -> str:
    return "\n".join(part for part in parts if part)


def trim_output(label: str, text: str, limit: int = 20) -> list[str]:
    lines = [f"{label}: {line}" for line in text.strip().splitlines()]
    if len(lines) <= limit:
        return lines
    head = lines[:limit]
    head.append(f"{label}: ... truncated {len(lines) - limit} lines")
    return head


def run_test(test: TestCase, args: argparse.Namespace) -> RunResult:
    if test.skip:
        summary = test.skip_reason or "skipped"
        return RunResult(True, (summary,), skipped=True)
    if args.esbmc and "--enforce-contract" in test.translator_flags:
        return RunResult(True, ("contract enforcement needs CBMC's DFCC",), skipped=True)
    if args.esbmc and test.esbmc_skip:
        return RunResult(True, (test.esbmc_skip,), skipped=True)
    goto_path = test.path.with_suffix(".goto")
    mode_flags: tuple[str, ...] = ()
    if args.esbmc:
        mode_flags = ("--esbmc",)
    elif args.goto_convert:
        mode_flags = ("--goto-convert",)
    tool, translate = translate_test(test, args, goto_path, mode_flags)
    if test.expected == "TRANSLATION FAILED":
        return expect_translation_failure(translate)
    if translate.returncode != 0:
        return RunResult(False, command_details(tool, translate))
    if args.esbmc:
        esbmc = run_esbmc(test, args, goto_path)
        output = merge_outputs(esbmc.stdout, esbmc.stderr)
        # ESBMC verifies bodiless functions vacuously; a missing body must fail the test.
        vacuous = "no body for function" in output or "no entry point to bridge" in output
        if esbmc.returncode not in {0, 1} or vacuous or test.expected not in output:
            return RunResult(False, command_details("esbmc", esbmc))
        return RunResult(True)
    # DFCC's instrumented output fails validation even for CBMC's own conversion.
    validate = args.goto_convert and selected_enforcement(test) is None
    tool, cbmc = verify_cbmc(test, args, goto_path, ("--validate-goto-model",) if validate else ())
    if not verified(tool, cbmc) or test.expected not in merge_outputs(cbmc.stdout, cbmc.stderr):
        return RunResult(False, command_details(tool, cbmc))
    if args.goto_convert:
        # Differential check: CBMC's own conversion of the symbol-table-only output is the oracle.
        reference_path = test.path.with_suffix(".symtab.goto")
        tool, reference = translate_test(test, args, reference_path)
        if reference.returncode == 0:
            tool, reference = verify_cbmc(test, args, reference_path, ())
        if not verified(tool, reference):
            return RunResult(False, command_details(tool, reference))
        expected, actual = property_results(reference.stdout), property_results(cbmc.stdout)
        if expected != actual:
            diff = sorted(((expected - actual) + (actual - expected)).elements())
            return RunResult(False, (f"property results differ (reference ^ converted): {diff}",))
    return RunResult(True)


def verify_cbmc(
    test: TestCase, args: argparse.Namespace, goto_path: Path, extra: tuple[str, ...]
) -> tuple[str, subprocess.CompletedProcess[str]]:
    """Run CBMC on `goto_path`, after DFCC instrumentation when the test enforces a contract."""
    verification_path = goto_path
    enforcement = selected_enforcement(test)
    if enforcement is not None:
        tool, instrumented, verification_path = instrument_contract(
            test, args, goto_path, enforcement
        )
        if instrumented.returncode != 0:
            return (tool, instrumented)
    return ("cbmc", run_cbmc(test, args, verification_path, extra))


def verified(tool: str, result: subprocess.CompletedProcess[str]) -> bool:
    return tool == "cbmc" and result.returncode in {0, 10}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("filter", nargs="?", default="*")
    parser.add_argument(
        "--llvm2cprover",
        default=str(ROOT / "target" / "debug" / "llvm2cprover"),
    )
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--cbmc", default="cbmc")
    parser.add_argument("--goto-cc", default="goto-cc")
    parser.add_argument("--goto-instrument", default="goto-instrument")
    parser.add_argument("--esbmc-bin", default="esbmc")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--goto-convert",
        action="store_true",
        help="translate with --goto-convert and check CBMC results against CBMC's own conversion",
    )
    mode.add_argument("--esbmc", action="store_true", help="translate with --esbmc and verify with ESBMC")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    tests = discover_tests(args.filter)
    print("TAP version 14")
    if not tests:
        print("1..0 # SKIP no tests matched")
        return 0
    print(f"1..{len(tests)}")
    failures = 0
    for index, test in enumerate(tests, start=1):
        progress = test.label
        if test.description:
            progress = f"{progress}: {test.description}"
        print(f"# running {progress}", flush=True)
        result = run_test(test, args)
        status = "ok" if result.ok else "not ok"
        suffix = ""
        if result.skipped:
            suffix = f" # SKIP {result.details[0]}"
        elif test.todo:
            suffix = f" # TODO {test.todo}"
        print(f"{status} {index} - {test.label}{suffix}", flush=True)
        for line in result.details:
            print(f"# {line}", flush=True)
        if not result.ok and not test.todo:
            failures += 1
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
