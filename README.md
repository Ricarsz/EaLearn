# EaLearn

桌面端语言学习工具，Linux + Windows。同一套应用，换语言只换内容包。

**当前状态**：内核、内容流水线与桌面端界面可用；英语内容包为种子内容，TTS 与图片素材尚未生成。

## 结构

```
crates/core     领域内核：pack 模型、标注校验、题型判分、练习排程、FSRS 调度
crates/lang     语言适配：LanguageProfile trait + 英语 / 韩语实现
crates/store    SQLite 本地库：pack 登记、SRS 卡、作答、设置
crates/llm      兼容 OpenAI 协议的流式对话客户端（SSE、正文流式抽取、标注回贴校验）
crates/tts      预合成音频：可插拔引擎（espeak-ng / piper）+ 按内容 hash 命名缓存
crates/forge    内容生产 CLI：校验、分词标注、内容指纹
app             React 前端（练习 / 情景）
src-tauri       Tauri 壳层：命令转发与状态管理，不含业务逻辑
packs/en        英语内容包（种子内容）
docs/design.md  架构设计
```

设计原则：内容（pack）与程序解耦，判分与调度在内核而不在前端，运行时绝不调用 LLM 生成教学内容。

## 运行

```sh
cargo test --workspace            # 内核与内容流水线
cd app && pnpm install            # 前端依赖（只需一次）
./scripts/dev.sh                  # 桌面端（开发模式，须在仓库根目录）
```

首次运行会在用户数据目录建库并登记 `packs/` 下的语言包。
设置环境变量 `EALEARN_PACKS` 可指向别的 pack 目录。

## 已实现的练习流程

组卷（题型按素材现算、同节不重复）→ 作答 → 内核判分 → 答错自动入卡 →
FSRS-5 排下次复习。到期卡会优先排进组卷，卡种决定题型。

五个题型：拼句（词块可拖拽）、看图选词、听音选词、背诵卡（正面词、背面释义、
自评熟练度）、阅读理解。背诵卡让没有音频/图片素材的词也能练。

词库页列出所有卡片，按熟练度（由 FSRS 稳定性换算）和到期时间排序，可筛选
待复习 / 不熟、可移出复习。支持导入自定义词条（每行 `word<TAB>释义[<TAB>标签]`），
导入即入卡。

情景页有两条线：剧本对话图（点任意词可查义并入卡）和 LLM 自由聊（自备 API key，
流式打字，标注做回贴校验）。

音色、模型配置、API key 都在「设置」（侧栏底部），音色可逐条试听。

音频由 `forge audio` 预生成到 `packs/*/media/audio/`，练习时零网络。当前音色是
`piper:en_US-amy`；没有模型时 `espeak-ng` 兜底。

piper 音色从 [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices)
下载 `onnx` 与同名 `.onnx.json`，放进
`${XDG_DATA_HOME:-~/.local/share}/ealearn/piper/`（可用 `EALEARN_PIPER_DIR` 覆盖）：

```sh
base=https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium
curl -LO $base/en_US-amy-medium.onnx
curl -LO $base/en_US-amy-medium.onnx.json
```

界面左侧可按语言切换音色（espeak 内置音色 + 本机已下载的 piper 模型）。选的音色
和 pack 一致时直接读预生成音频，否则按需合成并缓存。

情景页的自由聊有 16 个话题可选（八卦、约会、闲聊、点餐、租房、看病、追剧、游戏……），
话题会连同样例场景一起进提示词。

LLM 供应商只由 `base_url` 决定，兼容任何 `/chat/completions` 实现，包括本地
ollama 与自建代理。API key 存在本地库 `settings` 表，设置页只回显末 4 位。

## 开发排查

- 改了前端却还显示旧界面：WebKitGTK 会把 dev 资源缓存在应用数据目录，删掉
  `~/.local/share/top.chase141.ealearn/WebKitCache` 再启动。
- 某个音色没声音：`forge say <voice> "测试文本" /tmp/t.wav` 直接合成一份出来对比。
  espeak-ng 的 wav 头部长度字段是占位值，代码里已统一修正。

## forge

```sh
forge validate packs/en          # 校验：标注对齐、题目、对话图、素材、内容指纹
forge hash packs/en              # 打印内容指纹，写进 manifest 的 content_hash
forge annotate en "text"         # 打印分词与标注
forge tokens en "text"           # 输出 token 的 JSON 数组，生成 pack 时内联
forge audio packs/en             # 预合成音频并写回 pack（--voice=piper:en_US-amy 换音色）
```

`validate` 的核心是标注对齐校验：所有 token 必须首尾相接铺满原文（空白除外），
且每块内容与 span 指向的原文逐字节相等。LLM 生成的标注错位会在这里被挡下，
不会流入用户练习。

## 内容包

见 `docs/design.md` §3。一个 pack 目录包含 `manifest.json` 与
`words.json` / `sentences.json` / `articles.json` / `dialogs.json`，
素材（预合成音频、图片）放在 `media/` 下，路径相对 pack 根目录。