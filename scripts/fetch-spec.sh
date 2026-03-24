#!/usr/bin/env bash
set -euo pipefail

# Fetch the OpenAPI 3.0 spec from a locally built Gitea instance.
# Builds Gitea from the myers-main worktree, starts it, fetches the spec,
# and saves it to gitea-api/openapi.v1.json.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_DIR="$(dirname "$SCRIPT_DIR")"
GITEA_DIR="${GITEA_WORKSPACE:-$(dirname "$WORKSPACE_DIR")/gitea-myers-main}"
SPEC_OUT="$WORKSPACE_DIR/gitea-api/openapi.v1.json"
PORT=3333

if [[ ! -d "$GITEA_DIR" ]]; then
    echo "Error: Gitea directory not found at $GITEA_DIR"
    echo "Set GITEA_WORKSPACE or ensure the myers-main worktree exists"
    exit 1
fi

echo "Building Gitea from $GITEA_DIR..."
cd "$GITEA_DIR"
TAGS="bindata sqlite sqlite_unlock_notify" make build 2>&1 | tail -3

# Create temp working directory so Gitea doesn't pollute the repo
TMPDIR=$(mktemp -d)
trap 'kill $GITEA_PID 2>/dev/null; rm -rf $TMPDIR' EXIT

echo "Starting Gitea on port $PORT..."
cd "$TMPDIR"
"$GITEA_DIR/gitea" web --port "$PORT" --custom-path "$TMPDIR/custom" &
GITEA_PID=$!

# Wait for Gitea to be ready
echo -n "Waiting for Gitea to start"
for i in $(seq 1 30); do
    if curl -sf "http://localhost:$PORT/api/v1/version" >/dev/null 2>&1; then
        echo " ready!"
        break
    fi
    echo -n "."
    sleep 1
done

if ! curl -sf "http://localhost:$PORT/api/v1/version" >/dev/null 2>&1; then
    echo " failed!"
    echo "Gitea did not start in 30 seconds"
    exit 1
fi

echo "Fetching OpenAPI 3.0 spec..."
curl -sf "http://localhost:$PORT/openapi.v1.json" > "$SPEC_OUT"

echo "Spec saved to $SPEC_OUT ($(wc -c < "$SPEC_OUT") bytes)"

echo "Cleaning spec for progenitor compatibility..."
python3 "$SCRIPT_DIR/clean-spec.py" "$SPEC_OUT"

echo "Done."
