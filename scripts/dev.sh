#!/bin/sh
# 开发模式：启动 Vite 并拉起 Tauri 窗口。
# 必须在仓库根目录运行——Tauri CLI 会向下查找 src-tauri/tauri.conf.json。
set -e
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# WebKitGTK 会把 dev server 的响应缓存到应用数据目录，改完前端仍看到旧界面。
# 每次开发启动前清掉，省掉「明明改了却没生效」的排查。
cache="$HOME/.local/share/top.chase141.ealearn/WebKitCache"
[ -d "$cache" ] && rm -rf "$cache"

# 上一轮没退干净的实例会占着 1420，直接重开只会报 "Port already in use"
pkill -x ealearn-app 2>/dev/null || true
pid=$(ss -ltnp 2>/dev/null | grep ':1420' | grep -o 'pid=[0-9]*' | head -1 | cut -d= -f2)
[ -n "$pid" ] && kill "$pid" 2>/dev/null || true
sleep 1

exec ./app/node_modules/.bin/tauri dev
