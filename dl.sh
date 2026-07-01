#!/bin/sh
# fqdt 下载脚本 — 从 GitHub CI artifact 获取最新版
set -e

REPO="addallno/fqdt"
DEST="${1:-./fqdt}"

echo "  → 获取最新 CI artifact..."
RUN=$(curl -sf "https://api.github.com/repos/$REPO/actions/runs?per_page=1&status=success" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['workflow_runs'][0]['id'])" 2>/dev/null)

if [ -z "$RUN" ]; then
  echo "  err 无法获取最新 build"
  exit 1
fi

echo "  → 下载 artifact #$RUN ..."
gh run download "$RUN" -R "$REPO" -n fqdt-aarch64 -D /tmp/fqdt-dl 2>/dev/null || {
  curl -sfL "https://nightly.link/$REPO/actions/artifacts/$RUN.zip" -o /tmp/fqdt.zip
  unzip -o /tmp/fqdt.zip -d /tmp/fqdt-dl >/dev/null 2>&1
}

cp /tmp/fqdt-dl/fqdt "$DEST"
chmod +x "$DEST"
rm -rf /tmp/fqdt-dl /tmp/fqdt.zip

echo "  ok $DEST ($(ls -lh "$DEST" | awk '{print $5}'))"
