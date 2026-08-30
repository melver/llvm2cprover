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
