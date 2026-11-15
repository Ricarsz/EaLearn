#!/bin/sh
# 开发模式：启动 Vite 并拉起 Tauri 窗口。
# 必须在仓库根目录运行——Tauri CLI 会向下查找 src-tauri/tauri.conf.json。
set -e
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
exec ./app/node_modules/.bin/tauri dev
