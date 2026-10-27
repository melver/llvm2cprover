#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The llvm2cprover Authors

"""Differential fuzzing of llvm2cprover with Csmith.

The native -O0 checksum of a Csmith program is ground truth; the same program
lowered through clang -> llvm2cprover -> cbmc must prove it. Opt-in; not part
of `make`.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import asdict, dataclass, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SHIM = Path(__file__).resolve().parent / "shim"
CSMITH = shutil.which("csmith") or "csmith"
# The shim's `#include_next "csmith.h"` only resolves if the system header dir is on the path.
INCLUDES = ["-I", str(SHIM), "-I", str(Path(CSMITH).resolve().parent.parent / "include" / "csmith")]
CSMITH_ARGS = [
    "--no-argc", "--max-funcs", "6", "--max-block-depth", "3", "--max-block-size", "3",
    "--max-expr-complexity", "4", "--max-array-dim", "1", "--max-array-len-per-dim", "4",
    "--no-packed-struct",
]
# Worst first; a seed's outcome is the worst over its opt levels.
SEVERITY = ["semantic", "cbmc-error", "translate-crash", "check-failure", "unsupported",
            "unwind", "timeout", "invalid-seed", "pass"]
ACTIONABLE = {"semantic", "cbmc-error", "translate-crash", "check-failure"}
SHORT = {"check-failure": "check", "translate-crash": "crash", "cbmc-error": "cbmc-err",
         "unsupported": "unsup", "invalid-seed": "invalid"}
MAIN_END = "platform_main_end(crc32_context ^ 0xFFFFFFFFUL, print_hash_value);"
INJECT_RE = re.compile(re.escape(MAIN_END) + r"( __CPROVER_assert\(.*\);)?")
FAILURE_RE = re.compile(r"^\[(\S+)\] (?:(?:file \S+ )?line \d+ )?(.*): FAILURE$", re.M)


@dataclass(frozen=True)
class Config:
    llvm2cprover: str
    opts: tuple[str, ...]
    unwind: int
    cbmc_timeout: int
    native_timeout: int
    csmith_args: tuple[str, ...]
    keep_all: bool = False


@dataclass(frozen=True)
class Outcome:
    seed: int
    cls: str
    opt: str = ""
    feature: str = ""
    detail: str = ""
    secs: float = 0.0

    @property
    def label(self) -> str:
        return f"{self.cls}:{self.feature}" if self.feature else self.cls

    def line(self) -> str:
        detail = f"{self.feature}: {self.detail}" if self.feature else self.detail
        return f"seed {self.seed:>7}  {self.opt:<3} {self.cls:<16} {detail}"


def worst(outcomes: list[Outcome]) -> Outcome:
    return min(outcomes, key=lambda o: SEVERITY.index(o.cls))


def run(cmd: list[str], cwd: Path, timeout: float | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def last_line(text: str) -> str:
    return text.strip().splitlines()[-1] if text.strip() else ""


# ============================== Per-seed pipeline ============================== #
# Each stage returns an Outcome to stop on, or None to continue.


def generate(seed: int, cfg: Config, d: Path) -> Outcome | None:
    csmith = run([CSMITH, "--seed", str(seed), *cfg.csmith_args], d)
    if csmith.returncode:
        return Outcome(seed, "invalid-seed", detail=f"csmith: {last_line(csmith.stderr)}")
    (d / "t.c").write_text(csmith.stdout)
    return None


def inject(seed: int, cfg: Config, d: Path) -> Outcome | None:
    """Build the native oracle and pin its checksum into t.c.

    Idempotent, so `check` re-derives the checksum from a reduced program rather
    than trusting a stale constant.
    """
    clang = run(["clang", "-O0", "-w", "-DFUZZ_NATIVE", *INCLUDES, "t.c", "-o", "native"], d)
    if clang.returncode:
        return Outcome(seed, "invalid-seed", detail=f"clang native: {last_line(clang.stderr)}")
    try:
        native = run(["./native"], d, timeout=cfg.native_timeout)
    except subprocess.TimeoutExpired:
        return Outcome(seed, "invalid-seed", opt="nat", detail="native timeout")
    match = re.search(r"checksum = ([0-9A-F]+)", native.stdout)
    if match is None:
        return Outcome(seed, "invalid-seed", detail=f"native exit {native.returncode}, no checksum")
    assertion = f' __CPROVER_assert((crc32_context ^ 0xFFFFFFFFUL) == 0x{match[1]}u, "checksum");'
    src, count = INJECT_RE.subn(MAIN_END + assertion, (d / "t.c").read_text())
    if count != 1:
        return Outcome(seed, "invalid-seed", detail=f"{count} injection sites")
    (d / "t.c").write_text(src)
    return None


def classify_translate(seed: int, opt: str, stderr: str) -> Outcome:
    try:
        diag = json.loads(last_line(stderr))
    except json.JSONDecodeError:
        diag = {}
    if str(diag.get("kind", "")).startswith("unsupported"):
        return Outcome(seed, "unsupported", opt, diag.get("unsupported_feature") or diag["kind"],
                       diag.get("message", ""))
    panic = re.search(r"^thread .* panicked at .*$", stderr, re.M)
    detail = panic[0] if panic else diag.get("message") or last_line(stderr)
    return Outcome(seed, "translate-crash", opt, detail=detail)


def classify_cbmc(seed: int, opt: str, log: str) -> Outcome:
    failures = FAILURE_RE.findall(log)
    if any(desc == "checksum" for _, desc in failures):
        return Outcome(seed, "semantic", opt, detail="checksum")
    real = [name for name, desc in failures if not desc.startswith("unwinding assertion")]
    if real:
        return Outcome(seed, "check-failure", opt, detail=real[0])
    if failures:
        return Outcome(seed, "unwind", opt, detail=failures[0][0])
    if "VERIFICATION SUCCESSFUL" in log:
        return Outcome(seed, "pass", opt)
    return Outcome(seed, "cbmc-error", opt, detail="no verdict")


def verify(seed: int, cfg: Config, d: Path, opt: str) -> Outcome:
    o = d / opt
    o.mkdir(exist_ok=True)
    clang = run(["clang", f"-{opt}", "-w", "-g", "-Xclang", "-disable-O0-optnone", "-emit-llvm",
                 "-S", *INCLUDES, "../t.c", "-o", "t.ll"], o)
    if clang.returncode:
        return Outcome(seed, "invalid-seed", opt, detail=f"clang: {last_line(clang.stderr)}")
    translate = run([cfg.llvm2cprover, "t.ll", "-o", "t.goto", "--diagnostics", "json"], o)
    (o / "translate.json").write_text(translate.stderr)
    if translate.returncode:
        return classify_translate(seed, opt, translate.stderr)
    # Csmith programs easily exceed CBMC's default 2^8 addressable objects.
    cbmc = ["cbmc", "t.goto", "--function", "main", "--unwind", str(cfg.unwind),
            "--unwinding-assertions", "--object-bits", "10"]
    try:
        result = run(cbmc, o, timeout=cfg.cbmc_timeout)
    except subprocess.TimeoutExpired:
        return Outcome(seed, "timeout", opt, detail="cbmc")
    (o / "cbmc.log").write_text(result.stdout + result.stderr)
    if result.returncode not in (0, 10):
        return Outcome(seed, "cbmc-error", opt, detail=last_line(result.stderr))
    outcome = classify_cbmc(seed, opt, result.stdout)
    if outcome.cls in ("semantic", "check-failure"):
        trace = run(cbmc + ["--trace"], o, timeout=cfg.cbmc_timeout)
        (o / "cbmc.trace").write_text(trace.stdout)
    return outcome


def pipeline(seed: int, cfg: Config, d: Path) -> Outcome:
    """Steps shared by `run` and `check`: oracle, inject, verify per opt level."""
    return inject(seed, cfg, d) or worst([verify(seed, cfg, d, opt) for opt in cfg.opts])


def run_seed(seed: int, cfg: Config, out: Path) -> Outcome:
    d = out / f"seed-{seed}"
    d.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    outcome = generate(seed, cfg, d) or pipeline(seed, cfg, d)
    if outcome.cls == "pass" and not cfg.keep_all:
        shutil.rmtree(d)
    return replace(outcome, secs=round(time.monotonic() - start, 1))


# =================================== Console =================================== #


def header(cfg: Config, jobs: int, cwd: Path) -> str:
    def version(*cmd: str) -> str:  # cwd matters: csmith drops platform.info there.
        return re.search(r"\d+\.\d+\.\d+", run(list(cmd), cwd).stdout).group()

    git = run(["git", "describe", "--always", "--dirty"], ROOT).stdout.strip()
    return (f"csmith {version(CSMITH, '--version')} | clang {version('clang', '--version')} | "
            f"cbmc {version('cbmc', '--version')} | llvm2cprover {git} | "
            f"unwind {cfg.unwind} | -j{jobs}")


def progress(done: list[Outcome], total: int, elapsed: float) -> str:
    counts = Counter(o.cls for o in done)
    cells = " ".join(f"{SHORT.get(c, c)} {counts[c]}" for c in ["pass", *SEVERITY[:-1]])
    return f"[{len(done)}/{total}] {cells}  {len(done) / max(elapsed, 1e-9):.1f}/s"


def summary(done: list[Outcome]) -> str:
    by_class: dict[str, list[Outcome]] = defaultdict(list)
    for o in done:
        by_class[o.cls].append(o)
    lines = []
    for cls in SEVERITY:
        if cls not in by_class:
            continue
        seeds = sorted(o.seed for o in by_class[cls])
        tail = f"  seeds {seeds}" if cls in ACTIONABLE else ""
        lines.append(f"{cls:<16} {len(seeds)}{tail}")
    buckets = Counter((o.feature, o.detail) for o in by_class["unsupported"])
    lines.extend(f"  {n:>4}  {feature}: {detail}" for (feature, detail), n in buckets.most_common())
    return "\n".join(lines)


# ===================================== CLI ===================================== #


def config(args: argparse.Namespace) -> Config:
    return Config(
        llvm2cprover=args.llvm2cprover,
        opts=tuple((args.opt or "O0,O2").split(",")),
        unwind=args.unwind,
        cbmc_timeout=args.cbmc_timeout,
        native_timeout=args.native_timeout,
        csmith_args=(*CSMITH_ARGS, *args.csmith_arg),
        keep_all=args.keep_all,
    )


def cmd_run(args: argparse.Namespace) -> int:
    cfg = config(args)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    first, _, last = args.seeds.partition("-")
    seeds = range(int(first), int(last or first) + 1)
    print(header(cfg, args.jobs, out), flush=True)
    done: list[Outcome] = []
    start = time.monotonic()
    pool = ProcessPoolExecutor(args.jobs)
    try:
        with (out / "results.jsonl").open("a") as results:
            for future in as_completed(pool.submit(run_seed, s, cfg, out) for s in seeds):
                outcome = future.result()
                done.append(outcome)
                results.write(json.dumps(asdict(outcome)) + "\n")
                results.flush()
                if outcome.cls != "pass":
                    print(outcome.line(), flush=True)
                if len(done) % max(1, len(seeds) // 50) == 0:
                    print(progress(done, len(seeds), time.monotonic() - start), flush=True)
    except KeyboardInterrupt:
        pool.shutdown(wait=False, cancel_futures=True)
    print(summary(done))
    return 1 if any(o.cls in ACTIONABLE for o in done) else 0


def cmd_check(args: argparse.Namespace) -> int:
    outcome = pipeline(0, config(args), Path.cwd())
    print(outcome.line(), file=sys.stderr)
    return 0 if args.expect in (outcome.cls, outcome.label) else 1


def cmd_reduce(args: argparse.Namespace) -> int:
    d = Path(args.out) / f"seed-{args.seed}"
    results = (Path(args.out) / "results.jsonl").read_text().splitlines()
    outcome = Outcome(**next(json.loads(r) for r in results if json.loads(r)["seed"] == args.seed))
    check = [str(Path(__file__).resolve()), "check", "--opt", args.opt or outcome.opt,
             "--expect", outcome.label, "--llvm2cprover", args.llvm2cprover,
             "--unwind", str(args.unwind), "--cbmc-timeout", str(args.cbmc_timeout),
             "--native-timeout", str(args.native_timeout)]
    script = d / "interesting.sh"
    script.write_text("#!/bin/sh\nexec " + " ".join(check) + "\n")
    script.chmod(0o755)
    return subprocess.run(["creduce", str(script), "t.c"], cwd=d).returncode


def parse_args() -> argparse.Namespace:
    knobs = argparse.ArgumentParser(add_help=False)
    knobs.add_argument("--opt", help="comma-separated clang opt levels (default: O0,O2)")
    knobs.add_argument("--unwind", type=int, default=32)
    knobs.add_argument("--cbmc-timeout", type=int, default=120)
    knobs.add_argument("--native-timeout", type=int, default=10)
    knobs.add_argument("--llvm2cprover", default=str(ROOT / "target" / "debug" / "llvm2cprover"))
    knobs.add_argument("--csmith-arg", action="append", default=[])
    knobs.set_defaults(keep_all=False)
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("run", parents=[knobs], help="fuzz a seed range")
    p.add_argument("--out", required=True)
    p.add_argument("--seeds", required=True, help="A-B or A")
    p.add_argument("-j", "--jobs", type=int, default=1)
    p.add_argument("--keep-all", action="store_true")
    p.set_defaults(func=cmd_run)
    p = sub.add_parser("check", parents=[knobs], help="classify ./t.c; exit 0 iff --expect matches")
    p.add_argument("--expect", required=True)
    p.set_defaults(func=cmd_check)
    p = sub.add_parser("reduce", parents=[knobs],
                       help="creduce seed-S/t.c against its recorded class and opt level")
    p.add_argument("--out", required=True)
    p.add_argument("--seed", type=int, required=True)
    p.set_defaults(func=cmd_reduce)
    return parser.parse_args()


if __name__ == "__main__":
    ARGS = parse_args()
    raise SystemExit(ARGS.func(ARGS))
