#!/usr/bin/env bash
# 把 books/index.json 引用的所有资源抓成本地副本，并把 url 改写成自建仓地址。
#
#   scripts/mirror-books.sh https://gitee.com/<user>/<repo>/raw/master
#   scripts/mirror-books.sh https://example.com/ealearn --out /tmp/mirror
#
# 产物：
#   books/index.json                  url → <base>/books/<kind>/<id>.<ext>，原地址留作 mirrors
#   books/<kind>/<id>.<ext>           词表 / 句库 / 文章本身
#
# 之后把整个 books/ 推到自建仓，应用里把索引地址指到 <base>/books/index.json 即可。
set -euo pipefail

base=${1:?用法: scripts/mirror-books.sh <raw-base> [--out DIR]}
out="."
if [ "${2:-}" = "--out" ] && [ -n "${3:-}" ]; then out=$3; fi

root=$(cd "$(dirname "$0")/.." && pwd)
src="$root/books/index.json"
[ -f "$src" ] || { echo "找不到 $src" >&2; exit 1; }

mkdir -p "$out/books"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

echo '{"version": 1, "books": [' > "$tmp"
n=$(jq '.books | length' "$src")
i=0
failed=0
while [ "$i" -lt "$n" ]; do
  book=$(jq -c ".books[$i]" "$src")
  id=$(jq -r '.id' <<<"$book")
  kind=$(jq -r '.kind // "words"' <<<"$book")
  url=$(jq -r '.url' <<<"$book")

  case "$kind" in
    words)
      path=${url%%\?*}
      ext=${path##*.}
      [ "$ext" = "$path" ] && ext=json
      ;;
    sentences) ext=tsv ;;
    article|articles) ext=md ;;
    *) ext=json ;;
  esac
  [ -z "$ext" ] && ext=json

  dir="$out/books/$kind"
  mkdir -p "$dir"
  dest="$dir/$id.$ext"
  echo "↓ $id ← $url" >&2
  if ! curl -fsSL --max-time 180 "$url" -o "$dest"; then
    echo "  ✗ $id 下载失败，原样保留（url 不变，加进 mirrors）" >&2
    failed=$((failed + 1))
    jq -c --arg o "$url" '.mirrors = ((.mirrors // []) + [$o] | unique)' <<<"$book" >> "$tmp"
    [ "$i" -lt "$((n - 1))" ] && echo ',' >> "$tmp"
    i=$((i + 1))
    continue
  fi
  bytes=$(wc -c < "$dest")

  new_url="$base/books/$kind/$id.$ext"
  jq -c --arg u "$new_url" --arg o "$url" \
        '.url = $u | .mirrors = ((.mirrors // []) + [$o] | unique)' <<<"$book" >> "$tmp"
  [ "$i" -lt "$((n - 1))" ] && echo ',' >> "$tmp"
  printf '%s: %s (%s bytes)\n' "$id" "$dest" "$bytes" >&2
  i=$((i + 1))
done
echo ']}' >> "$tmp"

jq '.' "$tmp" > "$out/books/index.json"
if [ "$failed" -gt 0 ]; then
  echo "索引已写入 $out/books/index.json（$n 本，其中 $failed 本下载失败）" >&2
  exit 1
fi
echo "索引已写入 $out/books/index.json（$n 本）" >&2