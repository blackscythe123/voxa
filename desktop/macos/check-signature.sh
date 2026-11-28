#!/usr/bin/env bash
# The guard that keeps macOS Accessibility grants alive across updates. Runs on macOS after signing.
#
# macOS stores each grant against the app's designated requirement. If it changes, the switch in
# System Settings can still look on while access is silently denied. So:
#  - once desktop/macos/designated-requirement.txt holds a "designated =>" line, the built app's
#    requirement must equal it exactly, or the build fails;
#  - before that (no stable certificate yet), the app must be ad hoc signed.
set -euo pipefail
APP="${1:?usage: check-signature.sh path/to/Voxa.app}"
HERE="$(cd "$(dirname "$0")" && pwd)"
EXPECTED_FILE="$HERE/designated-requirement.txt"

codesign --verify --deep --strict "$APP"
ACTUAL=$(codesign -d -r- "$APP" 2>&1 | grep '^designated =>' || true)
echo "designated requirement: ${ACTUAL:-none}"
EXPECTED=$(grep '^designated =>' "$EXPECTED_FILE" 2>/dev/null || true)

if [ -n "$EXPECTED" ]; then
  if [ "$ACTUAL" != "$EXPECTED" ]; then
    echo "::error::The designated requirement changed. Every Mac user would lose the Accessibility grant." >&2
    echo "expected: $EXPECTED" >&2
    echo "actual:   $ACTUAL" >&2
    exit 1
  fi
  echo "ok: designated requirement unchanged"
else
  if codesign -dv "$APP" 2>&1 | grep -q 'Signature=adhoc'; then
    echo "ok: ad hoc signed (no stable certificate recorded yet)"
  else
    echo "::warning::Signed with a certificate, but desktop/macos/designated-requirement.txt has no requirement yet. Commit this line to it: $ACTUAL"
  fi
fi
