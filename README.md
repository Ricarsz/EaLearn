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
cargo test --workspace                      # 内核与内容流水线
cd app && pnpm install && pnpm tauri dev    # 桌面端
```

首次运行会在用户数据目录建库并登记 `packs/` 下的语言包。
设置环境变量 `EALEARN_PACKS` 可指向别的 pack 目录。

## 已实现的练习流程

组卷（题型按素材现算、同节不重复）→ 作答 → 内核判分 → 答错自动入卡 →
FSRS-5 排下次复习。情景页有两条线：剧本对话图（点任意词可查义并入卡）和
LLM 自由聊（自备 API key，流式打字，标注做回贴校验）。

音频由 `forge audio` 预生成到 `packs/*/media/audio/`，练习时零网络。默认引擎
`espeak-ng`；改成 `piper` 只需换 `voice` 约定并把模型放进 `EALEARN_PIPER_DIR`。

LLM 供应商只由 `base_url` 决定，兼容任何 `/chat/completions` 实现，包括本地
ollama 与自建代理。API key 存在本地库 `settings` 表，设置页只回显末 4 位。

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