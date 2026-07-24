#!/usr/bin/env python3
"""One-shot migrator that rewrites capability YAMLs to the new parameters:
block introduced in the LLM extraction gateway spec.

Reads:
  - knowledge/capabilities/**/*.yaml
  - knowledge/queries/**/*.yaml

For each capability, resolves the query it points at, converts every query
parameter into a per-parameter policy, and rewrites the capability file to:
  - drop legacy required_parameters / optional_parameters / clarification.missing_parameters
  - add a parameters: block

Usage:
    python3 scripts/migrate_capability_policies.py --dry-run   # print changes
    python3 scripts/migrate_capability_policies.py             # write files

Convention (matches spec §5.1 and §7 examples):
  - date parameters (from_date, to_date, as_of, etc.): default business_today,
    required=false, fill_when_missing=true.
  - integer parameter `limit`: default unbounded, hard_cap taken from
    capability.guards.max_limit if set (else 10_000).
  - array_bigint parameter `office_ids`: default authorized_scope,
    required=false, user_may_override=false.
  - everything else: required=true, no default.

Manual review afterwards is expected. This script does the mechanical work.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import yaml


REPO_ROOT = Path(__file__).resolve().parents[1]
CAP_DIR = REPO_ROOT / "knowledge" / "capabilities"
QUERY_DIR = REPO_ROOT / "knowledge" / "queries"


TYPE_MAP = {
    "date": "date",
    "integer": "integer",
    "bigint": "integer",
    "array_bigint": "integer_array",
    "string": "string",
    "currency_code": "currency",
}


def load_queries() -> dict[str, dict]:
    index: dict[str, dict] = {}
    for path in QUERY_DIR.rglob("*.yaml"):
        with path.open() as f:
            doc = yaml.safe_load(f)
        if not isinstance(doc, dict) or "id" not in doc:
            continue
        index[doc["id"]] = doc
    return index


def build_policy(qparam: dict, cap: dict) -> tuple[str, dict]:
    name = qparam["name"]
    raw_type = qparam.get("type", "string")
    kind = TYPE_MAP.get(raw_type, "string")
    policy: dict = {"type": kind}

    if kind == "date":
        policy.update(
            required=False,
            default="business_today",
            fill_when_missing=True,
        )
    elif name == "limit" and kind == "integer":
        guards = cap.get("guards", {}) or {}
        cap_val = guards.get("max_limit") or 10_000
        policy.update(
            required=False,
            default="unbounded",
            hard_cap=int(cap_val),
        )
    elif name == "office_ids" and kind == "integer_array":
        policy.update(
            required=False,
            default="authorized_scope",
            user_may_override=False,
        )
    else:
        # Preserve query-side requiredness for everything unclassified.
        policy["required"] = bool(qparam.get("required", True))

    return name, policy


def migrate_capability(cap_path: Path, queries: dict[str, dict]) -> str | None:
    with cap_path.open() as f:
        raw = f.read()
    doc = yaml.safe_load(raw)
    if not isinstance(doc, dict):
        return None

    query = queries.get(doc.get("query_id"))
    if query is None:
        print(f"  ! {cap_path.relative_to(REPO_ROOT)}: query not found ({doc.get('query_id')})", file=sys.stderr)
        return None

    parameters: dict[str, dict] = {}
    for qparam in query.get("parameters", []) or []:
        name, policy = build_policy(qparam, doc)
        parameters[name] = policy

    # Rebuild the doc in a stable order: keep existing keys, drop legacy ones,
    # insert `parameters:` after `guards:` if present else at the end.
    LEGACY_KEYS = {"required_parameters", "optional_parameters"}
    new_doc: dict = {}
    for key, value in doc.items():
        if key in LEGACY_KEYS:
            continue
        if key == "clarification":
            # Preserve clarification block but drop `missing_parameters` list.
            if isinstance(value, dict):
                filtered = {k: v for k, v in value.items() if k != "missing_parameters"}
                if filtered:
                    new_doc[key] = filtered
            continue
        new_doc[key] = value
    new_doc["parameters"] = parameters

    return yaml.safe_dump(new_doc, sort_keys=False, allow_unicode=True, width=100)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true", help="print diffs only")
    args = parser.parse_args()

    queries = load_queries()
    if not queries:
        print("No queries loaded — abort.", file=sys.stderr)
        return 1

    changed = 0
    for cap_path in sorted(CAP_DIR.rglob("*.yaml")):
        rewritten = migrate_capability(cap_path, queries)
        if rewritten is None:
            continue
        current = cap_path.read_text()
        if rewritten == current:
            continue
        changed += 1
        if args.dry_run:
            print(f"--- {cap_path.relative_to(REPO_ROOT)} (would rewrite)")
        else:
            cap_path.write_text(rewritten)
            print(f"rewrote {cap_path.relative_to(REPO_ROOT)}")

    action = "would change" if args.dry_run else "changed"
    print(f"{action} {changed} capability files.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
