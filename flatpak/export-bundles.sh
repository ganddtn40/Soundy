#!/bin/bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <ostree repo> <tag> <out dir>" >&2
  exit 1
fi

REPO="$1"
TAG="$2"
OUT="$3"
APP=app.soundy.player
BRANCH=stable
RUNTIME_REPO=https://flathub.org/repo/flathub.flatpakrepo

mkdir -p "$OUT" "$REPO/tmp" "$REPO/refs/remotes" "$REPO/refs/mirrors"

ARCHES="$(ostree refs --repo="$REPO" | sed -n "s#^app/$APP/\([^/]*\)/$BRANCH\$#\1#p")"
if [ -z "$ARCHES" ]; then
  echo "no $APP refs in $REPO" >&2
  exit 1
fi

for arch in $ARCHES; do
  bundle="$OUT/soundy-$TAG-$arch.flatpak"
  echo "==> $bundle"
  flatpak build-bundle \
    --arch="$arch" \
    --runtime-repo="$RUNTIME_REPO" \
    "$REPO" "$bundle" "$APP" "$BRANCH"
done
