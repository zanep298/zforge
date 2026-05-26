#!/usr/bin/env bash
# Docker-based smoke test for the zforge git primitives + init scaffold.
#
# What it covers:
#   - cargo build + install inside a clean rust:1-slim container
#   - `zforge init` produces .claude/commands/zforge.md (the slash command)
#   - `zforge git check-clean` exits 0 on clean tree
#   - `zforge git init-branch <ID>` creates zforge/<ID> branch
#   - `zforge git commit-phase <ID> <phase>` commits with the expected message
#
# What it does NOT cover:
#   - Claude Code CLI integration (host-only)
#   - sub-agent dispatch
#   - real LLM prompt execution
#
# Usage:
#   scripts/docker-smoke.sh
#
# Exit code 0 = all assertions passed.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> Running smoke test in rust:1-slim container"
echo "    mounting $REPO_ROOT -> /src (read-only)"

docker run --rm \
  -v "$REPO_ROOT:/src:ro" \
  -w /work \
  rust:1-slim \
  bash -eu -c '
    set -o pipefail

    echo "==> Installing build deps"
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends git pkg-config libssl-dev ca-certificates >/dev/null

    echo "==> Copying source (mount is read-only)"
    cp -r /src /work/zforge
    cd /work/zforge

    echo "==> cargo install (this takes a minute on first run)"
    cargo install --path . --force --quiet
    export PATH=/usr/local/cargo/bin:$PATH
    zforge --version

    echo "==> Bootstrapping test project at /tmp/proj"
    mkdir /tmp/proj && cd /tmp/proj
    git init -q
    git config user.email "test@test"
    git config user.name "test"
    git config init.defaultBranch main
    git checkout -q -b main
    git commit --allow-empty -q -m init

    echo "==> zforge init --force --no-register"
    zforge init --force --no-register >/dev/null

    echo "==> Asserting slash command scaffold"
    test -f .claude/commands/zforge.md || { echo "FAIL: .claude/commands/zforge.md not scaffolded"; exit 1; }
    echo "  OK .claude/commands/zforge.md"

    echo "==> Committing init artifacts so tree is clean"
    git add .
    git commit -q -m setup

    echo "==> zforge git check-clean (expect: clean)"
    zforge git check-clean

    echo "==> zforge task import TASK-1 --flow spike"
    zforge task import TASK-1 --title "smoke test" --flow spike >/dev/null
    test -f .zforge/tasks/TASK-1/task.md || { echo "FAIL: task.md missing"; exit 1; }

    echo "==> Committing task import so tree is clean again"
    git add .
    git commit -q -m "import TASK-1"

    echo "==> zforge git init-branch TASK-1"
    zforge git init-branch TASK-1
    branch="$(git rev-parse --abbrev-ref HEAD)"
    [ "$branch" = "zforge/TASK-1" ] || { echo "FAIL: expected zforge/TASK-1, got $branch"; exit 1; }
    echo "  OK on zforge/TASK-1"

    echo "==> Writing spec.md + committing phase"
    echo "spec body" > .zforge/tasks/TASK-1/spec.md
    zforge git commit-phase TASK-1 spec
    subject="$(git log -1 --format=%s)"
    expected="zforge(spec): TASK-1 smoke test"
    [ "$subject" = "$expected" ] || { echo "FAIL: subject mismatch"; echo "  expected: $expected"; echo "  got:      $subject"; exit 1; }
    echo "  OK commit subject: $subject"

    echo "==> Verifying noop on second commit with no changes"
    before="$(git rev-parse HEAD)"
    zforge git commit-phase TASK-1 testspec
    after="$(git rev-parse HEAD)"
    [ "$before" = "$after" ] || { echo "FAIL: empty commit-phase changed HEAD"; exit 1; }
    echo "  OK noop preserves HEAD"

    echo "==> git log"
    git log --oneline

    echo "==> git branch"
    git branch

    echo
    echo "✓ all smoke checks passed"
  '
