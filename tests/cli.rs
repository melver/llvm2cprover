// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn input_file(contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "llvm2cprover-cli-{}-{}.ll",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos()
    ));
    fs::write(&path, contents).expect("failed to write test input");
    path
}

fn run(input: &std::path::Path, diagnostics: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"));
    command.arg("--input-format").arg("ll");
    if let Some(format) = diagnostics {
        command.arg("--diagnostics").arg(format);
    }
    command
        .arg(input)
        .output()
        .expect("failed to run llvm2cprover")
}

#[test]
fn json_reports_structured_unsupported_inline_asm() {
    let input = input_file(
        r#"
define void @f() !dbg !10 {
entry:
  call void asm sideeffect "nop", "~{memory}"(), !dbg !18
  ret void, !dbg !19
}

!llvm.dbg.cu = !{!0}
!llvm.module.flags = !{!2, !3}
!0 = distinct !DICompileUnit(language: DW_LANG_C11, file: !1, producer: "clang", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)
!1 = !DIFile(filename: "/tmp/inline_asm.c", directory: "")
!2 = !{i32 7, !"Dwarf Version", i32 5}
!3 = !{i32 2, !"Debug Info Version", i32 3}
!10 = distinct !DISubprogram(name: "f", scope: !1, file: !1, line: 3, type: !12, scopeLine: 3, spFlags: DISPFlagDefinition, unit: !0)
!12 = !DISubroutineType(types: !13)
!13 = !{null}
!18 = !DILocation(line: 4, column: 3, scope: !10)
!19 = !DILocation(line: 5, column: 3, scope: !10)
"#,
    );
    let output = run(&input, Some("json"));
    fs::remove_file(input).expect("failed to remove test input");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not UTF-8");
    assert_eq!(
        stderr.lines().count(),
        1,
        "expected one JSON object: {stderr}"
    );
    let diagnostic: Value = serde_json::from_str(stderr.trim()).expect("stderr was not JSON");
    assert_eq!(diagnostic["kind"], "unsupported_feature");
    assert_eq!(diagnostic["unsupported_feature"], "inline_asm");
    assert_eq!(diagnostic["symbol"], "f");
    assert_eq!(diagnostic["location"]["file"], "/tmp/inline_asm.c");
    assert_eq!(diagnostic["location"]["function"], "f");
    assert_eq!(diagnostic["location"]["start_line"], 4);
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("inline asm")
    );
}

#[test]
fn json_normalizes_generic_parse_errors() {
    let input = input_file("not LLVM IR\n");
    let output = run(&input, Some("json"));
    fs::remove_file(input).expect("failed to remove test input");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not UTF-8");
    assert_eq!(
        stderr.lines().count(),
        1,
        "expected one JSON object: {stderr}"
    );
    let diagnostic: Value = serde_json::from_str(stderr.trim()).expect("stderr was not JSON");
    assert_eq!(diagnostic["kind"], "error");
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("failed to parse LLVM IR")
    );
    assert!(diagnostic.get("unsupported_feature").is_none());
}

#[test]
fn text_diagnostics_remain_the_default() {
    let input = input_file("not LLVM IR\n");
    let output = run(&input, None);
    fs::remove_file(input).expect("failed to remove test input");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not UTF-8");
    assert!(
        stderr.starts_with("Error: failed to parse LLVM IR:"),
        "{stderr}"
    );
    assert!(serde_json::from_str::<Value>(&stderr).is_err());
}

#[test]
fn rename_prefix_rejects_contract_manifests() {
    let input = input_file(
        r#"
define i1 @requires_f() {
entry:
  ret i1 true
}
define void @f() {
entry:
  ret void
}
"#,
    );
    let manifest = input.with_extension("toml");
    fs::write(
        &manifest,
        r#"
[functions.f]
requires = "requires_f"
"#,
    )
    .expect("failed to write contract manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format")
        .arg("ll")
        .arg("--rename-prefix")
        .arg("old_")
        .arg("--contracts")
        .arg(&manifest)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");
    fs::remove_file(input).expect("failed to remove test input");
    fs::remove_file(manifest).expect("failed to remove contract manifest");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not UTF-8");
    assert!(
        stderr.contains("--rename-prefix cannot currently be combined with --contracts"),
        "{stderr}"
    );
}

#[test]
fn granular_unsound_report_inventories_translation_assumptions() {
    let input = input_file(
        r#"
source_filename = "/tmp/report.c"
target datalayout = "e-m:e-p:64:64-i64:64-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.va_start.p0(ptr)
declare void @llvm.va_end.p0(ptr)
declare i32 @missing()
declare i32 @unused()
declare i32 @contracted(i32)
declare void @__CPROVER_assert(i1, ptr)
declare ptr @malloc(i64)

define i32 @model(i32 %x) {
entry:
  ret i32 %x
}

define i32 @f(ptr %ap, ...) !dbg !10 {
entry:
  call void @llvm.va_start.p0(ptr %ap), !dbg !18
  %a = call i32 @missing(), !dbg !19
  %b = call i32 @contracted(i32 %a), !dbg !20
  call void @__CPROVER_assert(i1 true, ptr null), !dbg !20
  %allocation = call ptr @malloc(i64 1), !dbg !20
  call void @llvm.va_end.p0(ptr %ap), !dbg !21
  ret i32 %b, !dbg !22
}

!llvm.dbg.cu = !{!0}
!llvm.module.flags = !{!2, !3}
!0 = distinct !DICompileUnit(language: DW_LANG_C11, file: !1, producer: "clang", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)
!1 = !DIFile(filename: "/tmp/report.c", directory: "")
!2 = !{i32 7, !"Dwarf Version", i32 5}
!3 = !{i32 2, !"Debug Info Version", i32 3}
!10 = distinct !DISubprogram(name: "f", scope: !1, file: !1, line: 3, type: !12, scopeLine: 3, spFlags: DISPFlagDefinition, unit: !0)
!12 = !DISubroutineType(types: !13)
!13 = !{!14, !15}
!14 = !DIBasicType(name: "int", size: 32, encoding: DW_ATE_signed)
!15 = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: null, size: 64)
!18 = !DILocation(line: 4, column: 3, scope: !10)
!19 = !DILocation(line: 5, column: 11, scope: !10)
!20 = !DILocation(line: 6, column: 11, scope: !10)
!21 = !DILocation(line: 7, column: 3, scope: !10)
!22 = !DILocation(line: 8, column: 3, scope: !10)
"#,
    );
    let manifest = input.with_extension("toml");
    let goto = input.with_extension("goto");
    let report = input.with_extension("report.json");
    fs::write(
        &manifest,
        r#"
[functions.contracted]
model = "model"
"#,
    )
    .expect("failed to write contract manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format=ll")
        .arg("--unsound=varargs")
        .arg("--contracts")
        .arg(&manifest)
        .arg("--output")
        .arg(&goto)
        .arg("--report")
        .arg(&report)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "granular selection should not warn"
    );
    let report_json: Value = serde_json::from_slice(&fs::read(&report).expect("missing report"))
        .expect("invalid report");
    assert_eq!(report_json["schema_version"], 2);
    assert_eq!(report_json["machine_model"], "lp64");
    assert_eq!(report_json["target_triple"], "x86_64-unknown-linux-gnu");
    assert_eq!(
        report_json["data_layout"],
        "e-m:e-p:64:64-i64:64-n8:16:32:64-S128"
    );
    assert_eq!(
        report_json["undefined_functions"],
        serde_json::json!(["missing"])
    );
    assert_eq!(report_json["approximations"].as_array().unwrap().len(), 2);
    assert_eq!(report_json["approximations"][0]["feature"], "varargs");
    assert_eq!(report_json["approximations"][0]["function"], "f");
    assert_eq!(
        report_json["approximations"][0]["location"]["file"],
        "/tmp/report.c"
    );
    assert_eq!(report_json["contracts"][0]["function"], "contracted");
    assert_eq!(report_json["contracts"][0]["reliance"], "instrumentation");
    assert_eq!(report_json["contracts"][0]["model"], "model");
    assert_eq!(report_json["contracts"][0]["discharged"], false);

    for path in [&input, &manifest, &goto, &report] {
        fs::remove_file(path).expect("failed to remove test artifact");
    }
}

#[test]
fn report_distinguishes_contract_replacement_reliance() {
    let input = input_file(
        r#"
define i32 @target(i32 %x) {
entry:
  ret i32 %x
}
define i1 @ensures_target(i32 %x, i32 %result) {
entry:
  %same = icmp eq i32 %x, %result
  ret i1 %same
}
define i32 @main() {
entry:
  %result = call i32 @target(i32 1)
  ret i32 %result
}
"#,
    );
    let manifest = input.with_extension("toml");
    let goto = input.with_extension("goto");
    let report = input.with_extension("report.json");
    fs::write(
        &manifest,
        r#"
[functions.target]
ensures = "ensures_target"
assigns = []
"#,
    )
    .expect("failed to write contract manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format=ll")
        .arg("--contracts")
        .arg(&manifest)
        .arg("--replace-with-contract")
        .arg("target")
        .arg("--output")
        .arg(&goto)
        .arg("--report")
        .arg(&report)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value = serde_json::from_slice(&fs::read(&report).expect("missing report"))
        .expect("invalid report");
    assert_eq!(report_json["contracts"][0]["function"], "target");
    assert_eq!(report_json["contracts"][0]["reliance"], "replacement");
    assert_eq!(
        report_json["contracts"][0]["assigns"],
        serde_json::json!([])
    );
    assert_eq!(report_json["contracts"][0]["discharged"], false);

    for path in [&input, &manifest, &goto, &report] {
        fs::remove_file(path).expect("failed to remove test artifact");
    }
}

#[test]
fn report_inventories_applied_inline_asm_models() {
    let input = input_file(
        r#"
define i32 @model(i32 %value) {
entry:
  ret i32 %value
}

define i32 @f(i32 %value) {
entry:
  %result = call i32 asm sideeffect "opaque", "=r,r"(i32 %value)
  ret i32 %result
}
"#,
    );
    let manifest = input.with_extension("toml");
    let goto = input.with_extension("goto");
    let report = input.with_extension("report.json");
    fs::write(
        &manifest,
        r#"
[[inline_asm]]
asm = "opaque"
constraints = "=r,r"
dialect = "att"
side_effects = true
align_stack = false
model = "model"
"#,
    )
    .expect("failed to write inline asm manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format=ll")
        .arg("--contracts")
        .arg(&manifest)
        .arg("--report")
        .arg(&report)
        .arg("--output")
        .arg(&goto)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_json: Value = serde_json::from_slice(&fs::read(&report).expect("missing report"))
        .expect("invalid report");
    assert_eq!(report_json["schema_version"], 2);
    assert_eq!(report_json["inline_asm_models"][0]["asm"], "opaque");
    assert_eq!(report_json["inline_asm_models"][0]["constraints"], "=r,r");
    assert_eq!(report_json["inline_asm_models"][0]["dialect"], "att");
    assert_eq!(report_json["inline_asm_models"][0]["side_effects"], true);
    assert_eq!(report_json["inline_asm_models"][0]["align_stack"], false);
    assert_eq!(report_json["inline_asm_models"][0]["function"], "f");
    assert_eq!(report_json["inline_asm_models"][0]["model"], "model");
    assert_eq!(report_json["inline_asm_models"][0]["discharged"], false);

    for path in [&input, &manifest, &goto, &report] {
        fs::remove_file(path).expect("failed to remove test artifact");
    }
}

#[test]
fn bare_unsound_warning_does_not_corrupt_json_failures() {
    let input = input_file(
        r#"
define void @f() {
entry:
  call void asm sideeffect "nop", "~{memory}"()
  ret void
}
"#,
    );
    let report = input.with_extension("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format=ll")
        .arg("--unsound")
        .arg("--diagnostics=json")
        .arg("--report")
        .arg(&report)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");
    fs::remove_file(input).expect("failed to remove test input");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not UTF-8");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    let diagnostic: Value = serde_json::from_str(stderr.trim()).expect("stderr was not JSON");
    assert_eq!(diagnostic["unsupported_feature"], "inline_asm");
    assert!(!stderr.contains("Warning:"), "{stderr}");
    assert!(
        !report.exists(),
        "failed translation must not emit a report"
    );
}

#[test]
fn bare_unsound_warns_after_success() {
    let input = input_file(
        r#"
define void @f(ptr %ap, ...) {
entry:
  call void @llvm.va_start.p0(ptr %ap)
  ret void
}
declare void @llvm.va_start.p0(ptr)
"#,
    );
    let goto = input.with_extension("goto");
    let output = Command::new(env!("CARGO_BIN_EXE_llvm2cprover"))
        .arg("--input-format=ll")
        .arg("--unsound")
        .arg("--output")
        .arg(&goto)
        .arg(&input)
        .output()
        .expect("failed to run llvm2cprover");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("bare --unsound"));
    fs::remove_file(input).expect("failed to remove test input");
    fs::remove_file(goto).expect("failed to remove goto output");
}
