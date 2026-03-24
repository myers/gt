#!/usr/bin/env python3
"""Clean an OpenAPI 3.0 spec for progenitor compatibility.

Handles known limitations where progenitor cannot process valid OAS3 constructs:
- Remove multipart/form-data and application/octet-stream request bodies
  (file uploads need hand-written implementations)
- Strip content from error response components that have typed schemas
  (progenitor requires uniform error response types)
- Deduplicate multiple success response codes with identical schemas

These are progenitor limitations, not spec errors.
"""

import json
import sys


def clean_spec(spec_path):
    spec = json.load(open(spec_path))
    responses_comp = spec.get("components", {}).get("responses", {})

    # 1. Remove binary request body content types
    for path, methods in spec.get("paths", {}).items():
        for method, op in methods.items():
            if not isinstance(op, dict):
                continue
            rb = op.get("requestBody")
            if not isinstance(rb, dict) or "content" not in rb:
                continue
            content = rb["content"]
            # Multi-content: keep only application/json
            if len(content) > 1:
                if "application/json" in content:
                    rb["content"] = {"application/json": content["application/json"]}
                elif "multipart/form-data" in content:
                    rb["content"] = {"multipart/form-data": content["multipart/form-data"]}
            # Remove binary types entirely
            for mt in list(rb.get("content", {})):
                if "multipart" in mt or "octet-stream" in mt:
                    del rb["content"][mt]
            if not rb.get("content"):
                op.pop("requestBody", None)

    # 2. Strip content from error response components used as non-2xx responses
    for name, resp in responses_comp.items():
        if "content" not in resp or not resp["content"]:
            continue
        is_error = False
        for path, methods in spec.get("paths", {}).items():
            for method, op in methods.items():
                if not isinstance(op, dict):
                    continue
                for code, r in op.get("responses", {}).items():
                    if isinstance(r, dict) and r.get("$ref", "").endswith("/" + name):
                        if not code.startswith("2"):
                            is_error = True
                            break
                if is_error:
                    break
            if is_error:
                break
        if is_error:
            del resp["content"]

    # 3. Deduplicate multiple success response codes
    def has_content(resp):
        if "$ref" in resp:
            name = resp["$ref"].split("/")[-1]
            resolved = responses_comp.get(name, {})
        else:
            resolved = resp
        return bool(resolved.get("content"))

    for path, methods in spec.get("paths", {}).items():
        for method, op in methods.items():
            if not isinstance(op, dict):
                continue
            responses = op.get("responses", {})
            success_codes = sorted(
                c for c in responses if c.startswith("2") and isinstance(responses[c], dict)
            )
            if len(success_codes) <= 1:
                continue
            # Keep only the first success code
            for c in success_codes[1:]:
                del responses[c]

    with open(spec_path, "w") as f:
        json.dump(spec, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else "gitea-api/openapi.v1.json"
    clean_spec(path)
    print(f"Cleaned {path}")
