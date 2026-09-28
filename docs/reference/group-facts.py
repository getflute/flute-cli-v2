#!/usr/bin/env python3
"""Derive the per-command-group API facts from the vendored bundle.

A schema fact written from memory goes wrong quietly: the wrong address keys,
a miscounted `requestBody.required`, the singular-versus-plural spelling of a
response example, request leaves missing from a count that is otherwise right.
Deriving each fact from the bundle removes that whole class.

`tests/conformance.rs` cross-checks its output against the surface walker. It
states what the bundle declares; it says nothing about what the live API
accepts.

Usage:
    group-facts.py                 regenerate group-facts.json
    group-facts.py --check         fail if the checked-in output is stale
"""

import argparse
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
BUNDLE = HERE / "openapi-v2.json"
DATA = HERE / "group-facts.json"

# The one path whose command group its own segments do not name.
TOKEN_PATH = "/oauth2/token"
TOKEN_GROUP = "auth"
METHODS = ("get", "put", "post", "delete", "options", "head", "patch", "trace")


def load():
    raw = BUNDLE.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def resolve(doc, node):
    """Follow `$ref` to the node it names."""
    seen = 0
    while isinstance(node, dict) and "$ref" in node:
        seen += 1
        if seen > 32:
            raise SystemExit(f"$ref chain too deep at {node['$ref']}")
        cur = doc
        for part in node["$ref"].lstrip("#/").split("/"):
            cur = cur[part]
        node = cur
    return node


def is_array(schema):
    """A nullable array's type is a list, so equality against the string
    silently stops the walk — the defect that once reduced the surface matrix
    to container rows."""
    t = schema.get("type")
    return t == "array" or (isinstance(t, list) and "array" in t)


def leaves(doc, schema, prefix="", seen=frozenset()):
    """Leaf pointers under `schema`, matching `tests/support/spec.rs`.

    Follows `$ref` with a visited set, descends `items` with a `/[]` step,
    unions `allOf`/`oneOf`/`anyOf`, and treats a schema with no leaves of its
    own as a leaf so a free-form object cannot vanish.
    """
    if isinstance(schema, dict) and "$ref" in schema:
        key = prefix + "#" + schema["$ref"]
        if key in seen:
            return [prefix]
        return leaves(doc, resolve(doc, schema), prefix, seen | {key})

    out = []
    for branch in ("allOf", "oneOf", "anyOf"):
        for alternative in schema.get(branch, []):
            out += leaves(doc, alternative, prefix, seen)

    if "properties" in schema:
        for name, sub in schema["properties"].items():
            out += leaves(doc, sub, f"{prefix}/{name}", seen)
    elif is_array(schema):
        items = schema.get("items")
        if isinstance(items, dict) and items:
            out += leaves(doc, items, prefix + "/[]", seen)
        else:
            out.append(prefix + "/[]")

    if not out and prefix:
        out = [prefix]
    return sorted(set(out))


def operations(doc):
    for path, item in doc["paths"].items():
        if path.startswith("/v2/webhooks"):
            continue
        for method, op in item.items():
            if method not in METHODS or not isinstance(op, dict):
                continue
            yield method, path, op


def describe(doc, method, path, op):
    oid = op["operationId"]
    body = op.get("requestBody", {})
    schema_node = body.get("content", {}).get("application/json", {}).get("schema")
    schema = resolve(doc, schema_node) if schema_node else None

    query = []
    for raw in op.get("parameters", []):
        p = resolve(doc, raw)
        if p.get("in") != "query":
            continue
        s = p.get("schema", {})
        query.append(
            {
                "name": p["name"],
                "type": s.get("type"),
                "format": s.get("format"),
                "enum": s.get("enum"),
                "default": s.get("default"),
                "minimum": s.get("minimum"),
                "maximum": s.get("maximum"),
                "required": bool(p.get("required")),
            }
        )

    responses = []
    for status, resp in sorted(op.get("responses", {}).items()):
        if not status.isdigit() or not 200 <= int(status) < 300:
            continue
        content = resp.get("content", {}).get("application/json")
        responses.append(
            {
                "status": int(status),
                "bodyless": content is None,
                "schema": (content or {}).get("schema", {}).get("$ref", "").rsplit("/", 1)[-1]
                or None,
                "example": bool(content and "example" in content),
                "examples": sorted((content or {}).get("examples", {}).keys()),
            }
        )

    return {
        "operation_id": oid,
        "method": method.upper(),
        "path": path,
        "group": TOKEN_GROUP if path == TOKEN_PATH else path.split("/")[2],
        "content_types": sorted(body.get("content", {}).keys()),
        "request_body_required_flag": body.get("required"),
        "body_schema": (schema_node or {}).get("$ref", "").rsplit("/", 1)[-1] or None,
        "required_fields": sorted((schema or {}).get("required", [])),
        "additional_properties": (schema or {}).get("additionalProperties"),
        "leaves": leaves(doc, schema) if schema else [],
        "query": query,
        "responses": responses,
    }


def build(doc, sha):
    ops = [describe(doc, m, p, o) for m, p, o in operations(doc)]
    ops.sort(key=lambda o: (o["group"], o["path"], o["method"]))
    return {"bundle_sha256": sha, "operation_count": len(ops), "operations": ops}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true",
                    help="fail if the checked-in output is stale")
    args = ap.parse_args()

    doc, sha = load()
    data = build(doc, sha)
    blob = json.dumps(data, indent=1, sort_keys=True) + "\n"

    if args.check:
        if not DATA.exists() or DATA.read_text() != blob:
            print(f"stale generated facts: {DATA.name}\n"
                  f"run: python3 {pathlib.Path(__file__).name}", file=sys.stderr)
            return 1
        print(f"generated facts are current (bundle {sha[:12]})")
        return 0

    DATA.write_text(blob)
    print(f"wrote {DATA.name} for bundle {sha[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
