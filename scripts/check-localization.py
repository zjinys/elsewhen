#!/usr/bin/env python3
"""Check bilingual ARB contracts and inventory remaining Dart string candidates.

The inventory is review input, not an assertion that every Chinese string is UI.
Generated code, tests, bundled user/third-party content and machine protocols
have separate ownership in docs/development/localization-coverage.md.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent


def closing(text, start):
    depth = 1
    for index in range(start + 1, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError("unclosed ICU argument")


def arguments(text):
    """Parse arguments and select/plural branches, excluding branch text."""
    args = {}
    index = 0
    while index < len(text):
        if text[index] != "{":
            index += 1
            continue
        match = re.match(r"\{([A-Za-z_]\w*)\s*([,}])", text[index:])
        if not match:
            index += 1
            continue
        end = closing(text, index)
        name = match[1]
        body = text[index + len(match[0]):end]
        if match[2] == "}":
            args[name] = args.get(name, "value")
        else:
            kind, separator, branches = body.partition(",")
            kind = kind.strip()
            if kind in ("plural", "selectordinal", "select"):
                if not separator:
                    raise ValueError(f"{name}: missing ICU branches")
                args[name] = kind
                selectors = []
                position = 0
                while position < len(branches):
                    if not branches[position:].strip():
                        break
                    branch = re.match(r"\s*(=\d+|[\w-]+)\s*\{", branches[position:])
                    if not branch:
                        raise ValueError(f"{name}: malformed ICU branch")
                    selectors.append(branch[1])
                    opening = position + len(branch[0]) - 1
                    last = closing(branches, opening)
                    for child, child_kind in arguments(branches[opening + 1:last]).items():
                        if child != name or child_kind != "value":
                            args[child] = child_kind
                    position = last + 1
                if "other" not in selectors or len(set(selectors)) != len(selectors):
                    raise ValueError(f"{name}: missing other or duplicate ICU selector")
                if kind == "select":
                    args[name] = "select:" + ",".join(sorted(selectors))
                elif any(s not in {"zero", "one", "two", "few", "many", "other"}
                         and not s.startswith("=") for s in selectors):
                    raise ValueError(f"{name}: invalid plural category")
            else:
                args[name] = kind
        index = end + 1
    return args


def check_resources(catalogs=None):
    catalogs = catalogs if catalogs is not None else [json.loads((ROOT / f"ui/lib/l10n/app_{locale}.arb").read_text(encoding="utf-8"))
                for locale in ("zh", "en")]
    keys = [{key for key in c if not key.startswith("@")} for c in catalogs]
    errors = []
    if keys[0] != keys[1]:
        errors.append(f"resource keys differ: {sorted(keys[0] ^ keys[1])}")
    for key in sorted(keys[0] & keys[1]):
        try:
            if any(not isinstance(c[key], str) or not c[key].strip() for c in catalogs):
                raise ValueError("missing or empty message")
            parsed = [arguments(c[key]) for c in catalogs]
            # Chinese has no one/other noun inflection. It can interpolate a
            # numeric argument directly while English uses plural branches.
            normalized = [{name: "numeric" if kind in ("value", "plural", "selectordinal")
                           else kind for name, kind in p.items()} for p in parsed]
            if normalized[0] != normalized[1]:
                raise ValueError(f"arguments/ICU kind differ: {parsed}")
            metadata = catalogs[0].get("@" + key, {}).get("placeholders", {})
            if set(metadata) != set(parsed[0]):
                raise ValueError(f"placeholder metadata differs: {sorted(metadata)} / {sorted(parsed[0])}")
            for name, definition in metadata.items():
                if any(p[name] in ("plural", "selectordinal") for p in parsed) and definition.get("type") not in ("int", "num", "double"):
                    raise ValueError(f"{name}: plural argument must have a numeric type")
                if name in catalogs[1].get("@" + key, {}).get("placeholders", {}):
                    other = catalogs[1]["@" + key]["placeholders"][name]
                    if definition.get("type") != other.get("type"):
                        raise ValueError(f"{name}: placeholder types differ")
        except ValueError as error:
            errors.append(f"{key}: {error}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return False
    print(f"ARB: {len(keys[0])} matching zh/en keys; arguments, types and ICU branches valid")
    return True


# This lexer removes comments before collecting literals. Interpolated strings
# may produce more than one candidate; the report deliberately keeps locations
# for a human to classify instead of silently excluding whole application files.
TOKENS = re.compile(
    r"//[^\n]*|/\*.*?\*/|r?(?:'''[\s\S]*?'''|\"\"\"[\s\S]*?\"\"\"|'(?:\\.|[^'\\])*'|\"(?:\\.|[^\"\\])*\")",
    re.DOTALL,
)
HAN = re.compile(r"[\u3400-\u9fff]")


def inventory(destination):
    candidates = []
    for path in sorted((ROOT / "ui/lib").rglob("*.dart")):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT).as_posix()
        if "/generated.dart/" in relative or "/l10n/" in relative or path.name.endswith(".g.dart"):
            continue
        text = path.read_text(encoding="utf-8")
        for match in TOKENS.finditer(text):
            token = match[0]
            if token.startswith(("//", "/*")) or not HAN.search(token):
                continue
            candidates.append({"path": relative, "line": text.count("\n", 0, match.start()) + 1,
                               "literal": token})
    destination.write_text(json.dumps(candidates, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    counts = Counter(item["path"] for item in candidates)
    print(f"Dart inventory: {len(candidates)} candidates in {len(counts)} files → {destination}")
    for path, count in counts.most_common(12):
        print(f"  {count:3d} {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, help="write remaining string candidates as JSON")
    args = parser.parse_args()
    valid = check_resources()
    if args.inventory:
        inventory(args.inventory)
    return 0 if valid else 1


if __name__ == "__main__":
    sys.exit(main())
