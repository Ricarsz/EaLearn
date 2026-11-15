# EaLearn

桌面端语言学习工具，Linux + Windows。同一套应用，换语言只换内容包。

**当前状态**：内核与内容流水线可用（Rust），GUI 尚未开始。

## 结构

```
crates/core     领域内核：pack 模型、标注校验、题型判分、FSRS 调度（无 GUI 依赖）
crates/lang     语言适配：LanguageProfile trait + 英语 / 韩语实现
crates/forge    内容生产 CLI：校验、分词标注、内容指纹
packs/en        英语内容包（种子内容）
docs/design.md  架构设计
```

设计原则：内容（pack）与程序解耦，判分与调度在内核而不在前端，运行时绝不调用 LLM 生成教学内容。

## 构建与测试

```sh
cargo test --workspace
cargo build -p ealearn-forge
```

## forge

```sh
forge validate packs/en          # 校验：标注对齐、题目、对话图、素材、内容指纹
forge hash packs/en              # 打印内容指纹，写进 manifest 的 content_hash
forge annotate en "text"         # 打印分词与标注
forge tokens en "text"           # 输出 token 的 JSON 数组，生成 pack 时内联
```

`validate` 的核心是标注对齐校验：所有 token 必须首尾相接铺满原文（空白除外），
且每块内容与 span 指向的原文逐字节相等。LLM 生成的标注错位会在这里被挡下，
不会流入用户练习。

## 内容包

见 `docs/design.md` §3。一个 pack 目录包含 `manifest.json` 与
`words.json` / `sentences.json` / `articles.json` / `dialogs.json`，
素材（预合成音频、图片）放在 `media/` 下，路径相对 pack 根目录。