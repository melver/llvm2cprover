#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The llvm2cprover Authors

"""Run end-to-end translator tests and print TAP."""

from __future__ import annotations

import argparse
import fnmatch
import subprocess
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
) -> tuple[str, subprocess.CompletedProcess[str]]:
    contract_flags = contract_args(test)
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
                    *test.translator_flags,
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
        test.translator_flags,
    )


def run_cbmc(test: TestCase, args: argparse.Namespace, goto_path: Path) -> subprocess.CompletedProcess[str]:
    command = [args.cbmc, str(goto_path)]
    if test.entry:
        command.extend(["--function", test.entry])
    command.extend(test.cbmc_flags)
    return run_command(command, cwd=ROOT)


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
    goto_path = test.path.with_suffix(".goto")
    tool, translate = translate_test(test, args, goto_path)
    if test.expected == "TRANSLATION FAILED":
        return expect_translation_failure(translate)
    if translate.returncode != 0:
        return RunResult(False, command_details(tool, translate))
    verification_path = goto_path
    enforcement = selected_enforcement(test)
    if enforcement is not None:
        tool, instrumented, verification_path = instrument_contract(
            test, args, goto_path, enforcement
        )
        if instrumented.returncode != 0:
            return RunResult(False, command_details(tool, instrumented))
    cbmc = run_cbmc(test, args, verification_path)
    if cbmc.returncode not in {0, 10}:
        return RunResult(False, command_details("cbmc", cbmc))
    output = "\n".join(part for part in (cbmc.stdout, cbmc.stderr) if part)
    if test.expected in output:
        return RunResult(True)
    return RunResult(False, command_details("cbmc", cbmc))


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
            reason = test.skip_reason or "skipped"
            suffix = f" # SKIP {reason}"
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
