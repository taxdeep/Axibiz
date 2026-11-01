"""Inventory Rust UI entry points and literal actions; not a correctness proof.

Run from the workspace root: py -3.11 scripts/audit_ui.py
Dynamic menus/actions and business rules require the manual audit and regression tests.
This command reads source only and writes its generated inventory under target/.
"""
import argparse
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", default="target/ui-audit-0200/source-inventory.json")
args = parser.parse_args()
files = [root / "src/gui.rs", *sorted((root / "src/gui").glob("*.rs"))]
inventory = []
for path in files:
    if path.name == "design_preview.rs":
        continue
    source = path.read_text(encoding="utf-8")
    # Keep production code only; Rust test modules follow this annotation.
    source = re.sub(r"#\[cfg\(test\)\]\s*mod \w+\s*\{.*?(?=\n(?:enum |pub\(|impl |pub |struct )|\Z)", lambda match: "\n" * match[0].count("\n"), source, flags=re.S)
    functions = [
        {"name": match[1], "line": source.count("\n", 0, match.start()) + 1}
        for match in re.finditer(r"\bfn\s+(\w+)\s*\(", source)
    ]
    actions = [
        {"label": match[1], "line": source.count("\n", 0, match.start()) + 1}
        for match in re.finditer(
            r'(?:\bbutton|\bsmall_button|\bmenu_button|Button::new)\(\s*"([^"\n]+)"', source
        )
    ]
    inventory.append({"file": path.relative_to(root).as_posix(), "functions": functions, "literal_actions": actions})
output = root / args.output
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps({"method": "Text inventory; dynamic actions and tests are excluded. See docs/UI_INTERACTION_AUDIT.md.", "modules": inventory}, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps({"files": len(inventory), "functions": sum(len(m["functions"]) for m in inventory), "literal_actions": sum(len(m["literal_actions"]) for m in inventory), "output": str(output)}))
