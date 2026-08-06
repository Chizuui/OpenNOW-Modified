#!/usr/bin/env bash
# Rebuild patches/chizui-login-patch-v3.patch from the current branch state.
#
# The patch must always apply cleanly on the upstream android-native base and
# mirror what the current branch actually ships. This script reproduces the
# proven workflow so a refresh is one command:
#
#   1. Fresh upstream worktree at UPSTREAM_REF (default fe682431 = android-native tip)
#   2. Apply the previous v3 patch as base (backend + UI + known strings)
#   3. Overwrite the 5 backend files byte-for-byte from the current branch
#      (they are chizui-feature files kept identical to the patched state)
#   4. Re-run the M3 login string-extraction edits (idempotent)
#   5. git diff -> format-patch header -> patches/chizui-login-patch-v3.patch
#   6. Validate: git apply on a second fresh worktree + login-string parity check
#
# Usage:
#   ./scripts/rebuild_chizui_patch.sh                 # defaults
#   UPSTREAM_REF=<sha> ./scripts/rebuild_chizui_patch.sh
#
# Requirements: upstream android-native tip (fe682431) available locally.
# When you extract more login strings, add literal->stringResource pairs to
# scripts/m3_login_edits.py and re-run — the parity check at the end tells you
# whether the patched result still matches the current branch.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"          # android/ project root
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"         # git repo root (patches/, android/)

PATCH_OUT="$REPO_ROOT/patches/chizui-login-patch-v3.patch"
EDIT_SCRIPT="$SCRIPT_DIR/m3_login_edits.py"

UPSTREAM_REF="${UPSTREAM_REF:-fe682431}"
BASE_PATCH="${BASE_PATCH:-$PATCH_OUT}"               # previous v3 applied as base
WORKTREE_BASE="${WORKTREE_BASE:-/tmp/chizui-patch-rebuild}"
WORKTREE_CHECK="${WORKTREE_CHECK:-/tmp/chizui-patch-check}"
TMP_RAW="${WORKTREE_BASE}.raw.patch"

# Chizui-feature files kept byte-identical between current branch and patch.
BACKEND_FILES=(
  "android/app/build.gradle.kts"
  "android/app/src/main/java/com/opencloudgaming/opennow/GfnApi.kt"
  "android/app/src/main/java/com/opencloudgaming/opennow/Models.kt"
  "android/app/src/main/java/com/opencloudgaming/opennow/OpenNowViewModel.kt"
  "android/app/src/main/java/com/opencloudgaming/opennow/Persistence.kt"
)

cleanup() {
  git -C "$REPO_ROOT" worktree remove "$WORKTREE_BASE" --force 2>/dev/null || true
  git -C "$REPO_ROOT" worktree remove "$WORKTREE_CHECK" --force 2>/dev/null || true
  git -C "$REPO_ROOT" worktree prune 2>/dev/null || true
}
trap cleanup EXIT

echo "==> [1/6] fresh upstream worktree ($UPSTREAM_REF)"
rm -rf "$WORKTREE_BASE" "$WORKTREE_CHECK"
git -C "$REPO_ROOT" worktree add "$WORKTREE_BASE" "$UPSTREAM_REF" >/dev/null

echo "==> [2/6] apply base patch: $(basename "$BASE_PATCH")"
(cd "$WORKTREE_BASE" && git apply "$BASE_PATCH")

echo "==> [3/6] overwrite backend files from current branch"
for f in "${BACKEND_FILES[@]}"; do
  cp "$REPO_ROOT/$f" "$WORKTREE_BASE/$f"
done

echo "==> [4/6] re-apply M3 login string-extraction edits"
(cd "$WORKTREE_BASE" && python3 "$EDIT_SCRIPT")

echo "==> [5/6] generate patch with format-patch header"
(cd "$WORKTREE_BASE" && git diff) > "$TMP_RAW"
{
  echo 'From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001'
  echo 'From: Chizuui <desckun@gmail.com>'
  echo "Date: $(date -u +'%a, %d %b %Y %H:%M:%S +0000')"
  echo 'Subject: [PATCH] feat: chizui-login v3 with full M3 login polish'
  echo
  cat "$TMP_RAW"
} > "$PATCH_OUT"

echo "==> [6/6] validate on fresh worktree"
git -C "$REPO_ROOT" worktree add "$WORKTREE_CHECK" "$UPSTREAM_REF" >/dev/null
(cd "$WORKTREE_CHECK" && git apply "$PATCH_OUT")
echo "    git apply: OK ($(wc -l < "$PATCH_OUT") lines)"

patched_set() {
  grep -ho 'R.string.login_[a-z_]*' \
    "$WORKTREE_CHECK/android/app/src/main/java/com/opencloudgaming/opennow/OpenNowScreens.kt" \
    "$WORKTREE_CHECK/android/app/src/main/java/com/opencloudgaming/opennow/OpenNowSettingsPanels.kt" | sort -u
}
current_set() {
  grep -ho 'R.string.login_[a-z_]*' \
    "$ANDROID_DIR/app/src/main/java/com/opencloudgaming/opennow/OpenNowLoginScreens.kt" \
    "$ANDROID_DIR/app/src/main/java/com/opencloudgaming/opennow/OpenNowScreens.kt" \
    "$ANDROID_DIR/app/src/main/java/com/opencloudgaming/opennow/OpenNowSettingsPanels.kt" | sort -u
}
if diff <(current_set) <(patched_set) >/dev/null; then
  echo "    login stringResource parity: OK ($(current_set | wc -l) strings)"
else
  echo "    WARNING: patched login-string set differs from current branch:"
  diff <(current_set) <(patched_set) | sed 's/^/      /'
  echo "    -> Add new deltas to scripts/m3_login_edits.py and re-run."
  exit 1
fi

echo "==> done: $PATCH_OUT"
