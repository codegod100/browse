#!/usr/bin/env python3
"""Generate android/gleam/browse/prebuilt/browse.wat from declarative screens/labels."""

from __future__ import annotations

import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_WAT = ROOT / "android/gleam/browse/prebuilt/browse.wat"
OUT_WASM = ROOT / "android/gleam/browse/prebuilt/browse.wasm"

LABELS = {
    1: "Browse",
    2: "Open",
    3: "RID",
    4: "Back",
    5: "Files",
    6: "Commits",
    7: "README",
    8: "Local repos",
    9: "No repositories in local storage yet.",
    10: "No repos match this filter.",
    11: "Up",
    12: "(binary or too large)",
    13: "(empty tree)",
    14: "(no commits)",
    15: "Select a file to view its contents.",
    16: "Select a commit to inspect its diff.",
    17: "Changed files",
    18: "Diff",
    19: "Seed a repo with radicle, then it will show up here.",
    20: "Filter by typing in the RID field.",
    21: "Help",
    22: "(no file changes)",
    23: "Select a changed file to view the patch.",
    24: "Files",
    25: "About",
    26: "Patches",
    27: "Issues",
    28: "Jobs",
    29: "Copy RID",
    30: "Head",
    31: "Description",
    32: "Name",
    33: "Storage",
    34: "Local only — Browse does not fetch from the network.",
    35: "Vidya shell · Gleam screens · Radicle storage",
    36: "Repository",
    37: "History",
    38: "Tree",
    39: "Blob",
    40: "Markdown",
}

SCREEN_NAMES = {
    0: "enter",
    1: "viewing",
    2: "error",
    3: "noprof",
}

TAG_NAMES = {
    0: "meta",
    1: "title",
    2: "body",
    3: "repo_list",
    4: "button",
    5: "space",
    6: "status",
    7: "header",
    8: "card_open",
    9: "card_close",
    10: "slot",
    11: "tree_list",
    12: "md_body",
    13: "file_list",
    14: "commit_list",
    15: "repo_tabs",
}


def pack(tag: int, payload: int = 0) -> int:
    return payload * 16 + tag


def button(primary: int, msg: int, label_code: int = 0) -> int:
    return pack(4, primary * 65536 + msg * 256 + label_code)


def slot(style: int, id_: int) -> int:
    return pack(10, style * 256 + id_)


SCREENS = {
    0: [  # enter (chrome only; inventory is host-owned)
        (pack(7), "Browse"),
        (pack(5, 1), None),
        (pack(2), "Paste a Radicle ID (rad:z…) for a repo already in local storage, then Open."),
        (pack(5, 1), None),
        (pack(6), "Filter by typing in the RID field."),
        (pack(5, 1), None),
        (pack(6), "Local only — Browse does not fetch from the network."),
        (pack(5, 2), None),
    ],
    1: [  # viewing
        (pack(7), "Browse"),
        (pack(5, 1), None),
        (button(0, 1), "Back"),
        (pack(5, 2), None),
        (pack(8), None),
        (pack(1), "Repository"),
        (pack(5, 1), None),
        (pack(0), None),
        (pack(9), None),
        (pack(5, 2), None),
        (pack(15), None),
    ],
    2: [  # error
        (pack(7), "Browse"),
        (pack(5, 2), None),
        (pack(8), None),
        (pack(1), "Could not open"),
        (pack(6), "Check the RID and that the repo is seeded locally."),
        (pack(5, 1), None),
        (pack(2), "The host could not load this repository from your Radicle profile."),
        (pack(5, 1), None),
        (pack(2), "Confirm the ID starts with rad:z and that `rad` can see the repo."),
        (pack(5, 1), None),
        (slot(1, 5), None),
        (pack(5, 2), None),
        (button(1, 1), "Back"),
        (pack(9), None),
    ],
    3: [  # noprof
        (pack(7), "Browse"),
        (pack(5, 2), None),
        (pack(8), None),
        (pack(1), "No Radicle profile"),
        (pack(2), "Could not load ~/.radicle. Create a profile with radicle, then reopen Browse."),
        (pack(5, 1), None),
        (pack(6), "Browse only shows repositories already present in local storage."),
        (pack(5, 1), None),
        (pack(2), "After `rad auth` (or equivalent), restart Browse and seed a project."),
        (slot(2, 5), None),
        (pack(9), None),
    ],
}


def alloc_strings() -> tuple[dict[str, int], list[tuple[int, bytes]]]:
    ptr = 16
    empty = (ptr, struct.pack("<I", 0))
    ptr = 32
    mapping: dict[str, int] = {"": empty[0]}
    segs = [empty]
    ordered: list[str] = []
    for i in sorted(LABELS):
        ordered.append(LABELS[i])
    for i in sorted(SCREEN_NAMES):
        ordered.append(SCREEN_NAMES[i])
    for i in sorted(TAG_NAMES):
        ordered.append(TAG_NAMES[i])
    for ops in SCREENS.values():
        for _, text in ops:
            if text is not None:
                ordered.append(text)
    for s in ordered:
        if s in mapping:
            continue
        raw = s.encode("utf-8")
        mapping[s] = ptr
        segs.append((ptr, struct.pack("<I", len(raw)) + raw))
        ptr += 4 + len(raw)
        ptr = (ptr + 7) & ~7
    return mapping, segs


def emit_case_i64(
    indent: str, local: str, cases: list[tuple[int, str]], default: str = "(i64.const 0)"
) -> str:
    lines = [f"{indent}(block $out (result i64)"]
    for val, expr in cases:
        lines.append(
            f"{indent}  (if (i64.eq (local.get ${local}) (i64.const {val}))"
            f" (then (br $out {expr})))"
        )
    lines.append(f"{indent}  {default})")
    return "\n".join(lines)


def emit_case_i32(indent: str, local: str, cases: list[tuple[int, str]], default: str) -> str:
    lines = [f"{indent}(block $out (result i32)"]
    for val, expr in cases:
        lines.append(
            f"{indent}  (if (i64.eq (local.get ${local}) (i64.const {val}))"
            f" (then (br $out {expr})))"
        )
    lines.append(f"{indent}  {default})")
    return "\n".join(lines)


def emit_case_i32_from_i32(
    indent: str, local: str, cases: list[tuple[int, str]], default: str
) -> str:
    lines = [f"{indent}(block $out (result i32)"]
    for val, expr in cases:
        lines.append(
            f"{indent}  (if (i32.eq (local.get ${local}) (i32.const {val}))"
            f" (then (br $out {expr})))"
        )
    lines.append(f"{indent}  {default})")
    return "\n".join(lines)


def main() -> None:
    mapping, segs = alloc_strings()
    empty_ptr = mapping[""]

    lines: list[str] = []
    lines.append(";; AUTO-GENERATED by tools/gen_browse_wat.py — do not edit by hand.")
    lines.append(";; Mirrors android/gleam/browse/src/browse.gleam TEA + labels + view_text.")
    lines.append("(module")
    lines.append('  (memory (export "memory") 1)')
    lines.append("")
    for addr, blob in segs:
        esc = "".join(f"\\{b:02x}" for b in blob)
        lines.append(f'  (data (i32.const {addr}) "{esc}")')
    lines.append("")
    lines.append('  (func (export "gleam_string_utf8_len") (param $s i32) (result i32)')
    lines.append("    (i32.load (local.get $s)))")
    lines.append('  (func (export "gleam_string_utf8_ptr") (param $s i32) (result i32)')
    lines.append("    (i32.add (local.get $s) (i32.const 4)))")
    lines.append('  (func (export "__gleam_string_len") (param $s i32) (result i32)')
    lines.append("    (i32.load (local.get $s)))")
    lines.append('  (func (export "__gleam_string_data") (param $s i32) (result i32)')
    lines.append("    (i32.add (local.get $s) (i32.const 4)))")
    lines.append("")
    lines.append('  (func (export "browse__init") (result i64) (i64.const 0))')
    lines.append("")
    lines.append(
        '  (func (export "browse__update") (param $model i64) (param $msg i64) (result i64)'
    )
    lines.append("    (block $out (result i64)")
    for msg, model in [(1, 0), (2, 1), (3, 2), (4, 3)]:
        lines.append(
            f"      (if (i64.eq (local.get $msg) (i64.const {msg}))"
            f" (then (br $out (i64.const {model}))))"
        )
    lines.append("      (local.get $model))")
    lines.append("  )")
    lines.append("")

    len_cases = [(m, f"(i64.const {len(ops)})") for m, ops in SCREENS.items()]
    lines.append('  (func (export "browse__view_len") (param $model i64) (result i64)')
    lines.append(emit_case_i64("    ", "model", len_cases))
    lines.append("  )")
    lines.append("")

    for model, ops in SCREENS.items():
        at_cases = [(i, f"(i64.const {packed})") for i, (packed, _) in enumerate(ops)]
        lines.append(f"  (func $screen{model}_at (param $i i64) (result i64)")
        lines.append(emit_case_i64("    ", "i", at_cases))
        lines.append("  )")
        lines.append("")
        text_cases = []
        for i, (_, text) in enumerate(ops):
            ptr = mapping[text] if text is not None else empty_ptr
            text_cases.append((i, f"(i32.const {ptr})"))
        lines.append(f"  (func $screen{model}_text (param $i i64) (result i32)")
        lines.append(emit_case_i32("    ", "i", text_cases, f"(i32.const {empty_ptr})"))
        lines.append("  )")
        lines.append("")

    lines.append(
        '  (func (export "browse__view_at") (param $model i64) (param $i i64) (result i64)'
    )
    lines.append(
        emit_case_i64(
            "    ",
            "model",
            [(m, f"(call $screen{m}_at (local.get $i))") for m in SCREENS],
        )
    )
    lines.append("  )")
    lines.append("")
    lines.append(
        '  (func (export "browse__view_text") (param $model i64) (param $i i64) (result i32)'
    )
    lines.append(
        emit_case_i32(
            "    ",
            "model",
            [(m, f"(call $screen{m}_text (local.get $i))") for m in SCREENS],
            f"(i32.const {empty_ptr})",
        )
    )
    lines.append("  )")
    lines.append("")

    lines.append('  (func (export "browse__label") (param $id i32) (result i32)')
    lines.append(
        emit_case_i32_from_i32(
            "    ",
            "id",
            [(i, f"(i32.const {mapping[s]})") for i, s in sorted(LABELS.items())],
            f"(i32.const {empty_ptr})",
        )
    )
    lines.append("  )")
    lines.append("")

    lines.append('  (func (export "browse__screen_name") (param $model i64) (result i32)')
    lines.append(
        emit_case_i32(
            "    ",
            "model",
            [(m, f"(i32.const {mapping[n]})") for m, n in sorted(SCREEN_NAMES.items())],
            f"(i32.const {empty_ptr})",
        )
    )
    lines.append("  )")
    lines.append("")

    lines.append('  (func (export "browse__tag_name") (param $tag i64) (result i32)')
    lines.append(
        emit_case_i32(
            "    ",
            "tag",
            [(t, f"(i32.const {mapping[n]})") for t, n in sorted(TAG_NAMES.items())],
            f"(i32.const {empty_ptr})",
        )
    )
    lines.append("  )")
    lines.append(")")

    OUT_WAT.write_text("\n".join(lines) + "\n")
    print(f"wrote {OUT_WAT} ({OUT_WAT.stat().st_size} bytes)")
    subprocess.check_call(["wat2wasm", str(OUT_WAT), "-o", str(OUT_WASM)])
    print(f"wrote {OUT_WASM} ({OUT_WASM.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
