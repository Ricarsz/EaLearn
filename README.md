# EaLearn

桌面端语言学习工具，Linux + Windows。同一套应用，换语言只换内容包。

**当前状态**：内核、内容流水线、桌面端界面齐全；英语与韩语内容包都已接入，英语 29 句 / 4 篇阅读、韩语 19 句 / 3 篇。另有 120 句英语口语句库放在在线索引里，按需下载进本地库，不占 pack。种子句子带预合成音频，其余按需合成并缓存。

## 结构

```
crates/core     领域内核：pack 模型、标注校验、题型判分、组卷、FSRS 调度
crates/lang     语言适配：LanguageProfile trait + 英语 / 韩语实现
crates/store    SQLite 本地库：pack 登记、SRS 卡、作答、设置、聊天记录
crates/llm      兼容 OpenAI 协议的流式对话客户端（SSE、正文流式抽取、标注回贴校验）
crates/tts      piper 合成 + 按「音色 + 文本」hash 命名的 wav 缓存
crates/forge    内容 CLI：校验、分词标注、预合成音频、对话草稿生成
app             React 前端（首页 / 练习 / 情景 / 词库 / 发音 / 统计）
src-tauri       Tauri 壳层：命令转发与状态管理，不含业务逻辑
packs/en        英语内容包
packs/ko        韩语内容包
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

## 安装

发布版：GitHub Release 的 deb / rpm。Arch 从 `packaging/PKGBUILD` 构建：

```sh
cd packaging && makepkg -f && paru -U ealearn-<version>-1-x86_64.pkg.tar.zst
```

## 首页与练法

首页先选怎么练：混合复习、背单词、听力、拼句、阅读。当前语言包没有对应素材时该练法置灰。

- **混合复习**排在第一位：到期卡先排，剩下的位置用新素材补满，完成页会写明其中几题是复习
- 单项练法只出那一种题，不会混进别的题型

上面还有一张**今日目标**卡片：每天做多少题，或者每天专注多少分钟，二选一。目标存在本地
设置里，进度按当天的作答 / 番茄钟记录实时算。设定目标时可以绑定一本词书——按「目标 × 30 天」
估算需要多少词并推荐一本够用的，保存时若这本还没导入就自动下载导入。有题量目标时，
点进练习的一组直接按「今天还差多少题」出，一组练完就达标。

一组练完 → 内核判分 → 答错自动入卡 → FSRS-5 排下次复习。答对且没卡只记作答，不把每道题都塞进复习队列。

## 题型

四个题型：拼句（词块可拖拽）、听音选词、背诵卡、阅读理解（先出正文再出选择题）。
背诵卡正面只出词和发音，先让用户自评 认识 / 模糊 / 不认识 / 太简单，选完才显示释义——
先看答案再自评，评出来的「认识」不可信。没有音频素材的词也能靠它练。

情景页另有一条剧本对话线，走对话图分支，不算单题判分。

各模块的素材从哪来：

| 模块 | 素材 |
|---|---|
| 背单词 | pack 词条、导入的自定义词条、单词书 |
| 听力 | 同上；不要求预生成音频，播放时按需合成并缓存 |
| 拼句 | pack 的 `sentences.json`，或导入的多词短语（≥3 个词，按句子卡入队） |
| 阅读 | pack 的 `articles.json`，或词库页「导入文章」粘贴一篇 |

## 词库与单词书

词库页列出所有卡片，按熟练度和到期时间排序，可筛选待复习 / 不熟 / 自定义，
顶部搜索框按词面或释义过滤，几万张卡也不卡。自定义词条每行
`word<TAB>释义[<TAB>标签]`，导入即入卡。

单词书走 `books/index.json`：内置八本英语应试书（四级 2607 / 六级 2345 / 考研 6705 /
高考 3893 / 雅思 2644 / 托福 4123 / GRE 3041 / 核心 4000）和两本韩语书（入门 486 词、
5000 常用词 4725 词）。导入过的书标「已下载」，可以重复导入更新释义。

「导入文章」用纯文本约定写阅读材料：

```
# 标题
正文第一段
第二段

? 题干
- 选项 A
- 选项 B
= 2
```

`#` 标题、`?` 题干、`-` 选项、`=` 正确选项序号（从 1 开始），其余行是正文，空行分段。
正文会按语言分词对齐校验，对不上就整篇拒绝。

## 自建内容仓

想自己管内容、不依赖别人的词表地址，见 [docs/content-hosting.md](docs/content-hosting.md)。

## 情景

两条线：剧本对话图（点任意词可查义并入卡）和 LLM 自由聊（自备 API key，
流式打字，标注做回贴校验）。自由聊有 16 个话题可选（八卦、约会、闲聊、点餐、租房、看病、追剧、游戏……），
话题会连同样例场景一起进提示词。

LLM 供应商只由 `base_url` 决定，兼容任何 `/chat/completions` 实现，包括本地
ollama 与自建代理。设置里先「确认端点」（用这把 key 列出模型），确认成功才出现
模型下拉框；API key 明文存在本地库 `settings` 表，设置页只回显末 4 位。

## 发音

侧栏「发音」是静态参考表：英语 12 个单元音、8 个双元音、24 个辅音；韩语 19 个辅音、
21 个元音、7 个代表收音（받침）。韩语每个字母先听「字音」（ㄱ 读 그、ㅏ 读 아），
再听「例词」（가방）；英语每行听示例词。表只做查询，不参与判分。

## 音色

音频由 `forge audio` 预生成到 `packs/*/media/audio/`，练习时零网络。当前音色是
`piper:en_US-lessac`。合成只走 piper：没有对应模型时直接报错，不会退化成机械音。

音色也可以直接在应用里装：设置 → 发音 → 「更多音色（下载）」，列出 piper 官方索引里
这门语言的全部音色（英语 40 个、韩语 1 个），点下载即写入模型目录，随后在上面的列表里选中即可。
手动装的话，从 [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices)
下载 `onnx` 与同名 `.onnx.json`，放进 `${XDG_DATA_HOME:-~/.local/share}/ealearn/piper/`
（可用 `EALEARN_PIPER_DIR` 覆盖）：

```sh
base=https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium
curl -LO $base/en_US-lessac-medium.onnx
curl -LO $base/en_US-lessac-medium.onnx.json
```

设置里可按语言切换音色并逐条试听，列表只列本机真实存在的模型。选的音色和 pack
一致且词条有预生成音频时直接读文件，否则按需合成并缓存到用户数据目录。

## 数据

所有状态在一个 SQLite 文件里：`~/.local/share/top.chase141.ealearn/ealearn.db`（Windows 在应用数据目录）。
设置抽屉底部有两档清理：

- **清除学习记录**：卡片、复习进度、作答、番茄钟、聊天记录
- **清空所有数据**：再加导入的词条、文章和设置，回到刚装好的状态

设置 → 数据还有「导出 CSV」：把当前语言的卡片导出成
`word,gloss,kind,stability,difficulty,reps,lapses,due,last_review`，
默认写到应用数据目录的 `exports/ealearn-<lang>-<日期>.csv`，可以直接进表格或留给 Anki 备份。
「备份整库」用 `VACUUM INTO` 把整个 SQLite 复制到 `exports/ealearn-<日期>.db`，
包含导入的词书、文章和设置，换机时复制回去即可。

`packs/` 是只读内容，清理不会动它；语言包会在下次启动时重新登记。

## 统计与错题

侧栏「统计」：近 7/14/30 天作答柱状图、题型正确率与平均耗时、未来 7 天到期负荷。
错题本按错次排序，可以直接「错题重练」。

## 开发排查

- 改了前端却还显示旧界面：WebKitGTK 会把 dev 资源缓存在应用数据目录，删掉
  `~/.local/share/top.chase141.ealearn/WebKitCache` 再启动（`scripts/dev.sh` 每次会清）。
- 某个音色没声音：`forge say <voice> "测试文本" /tmp/t.wav` 直接合成一份出来对比。
- 音色列表是空的：说明模型目录里没有对应语言的 piper 模型，见上面的下载步骤。

## forge

```sh
forge validate packs/en          # 校验：标注对齐、题目、对话图、素材、内容指纹
forge hash packs/en              # 打印内容指纹，写进 manifest 的 content_hash
forge annotate en "text"         # 打印分词与标注
forge tokens en "text"           # 输出 token 的 JSON 数组，生成 pack 时内联
forge audio packs/en             # 预合成音频并写回 pack（--voice=piper:en_US-lessac 换音色）
forge voices en                  # 列出本机可用音色
forge say piper:en_US-lessac "hi" /tmp/t.wav   # 合成一句，排查音色问题
forge gen dialog en "被人放鸽子"  # LLM 出对话草稿到 drafts/（先 --dry-run 看提示词）
forge check-import book.json     # 试解析词书，报告能识别多少词
forge corpus words packs/en --from=ecdict.csv --tag=cet4 --limit=2000
                                 # 从 ECDICT 批量灌词条（按词频排序，带考试标签）
forge corpus sentences packs/en --from=cmn.txt --limit=200 --max-words=12
                                 # 从 Tatoeba 句对批量灌句子（英文<TAB>中文）
```

批量语料只用公开数据：词条与释义来自 [ECDICT](https://github.com/skywind3000/ECDICT)，
例句来自 Tatoeba（CC BY 2.0，句对文件可用 manythings.org 的 `cmn-eng.zip`）。
`corpus` 只做筛选、去重和分词标注，写完自己更新 content_hash，之后跑 `forge audio` 补音频。

`validate` 的核心是标注对齐校验：所有 token 必须首尾相接铺满原文（空白除外），
且每块内容与 span 指向的原文逐字节相等。LLM 生成的标注错位会在这里被挡下，
不会流入用户练习。

## 内容包

见 `docs/design.md` §3。一个 pack 目录包含 `manifest.json` 与
`words.json` / `sentences.json` / `articles.json` / `dialogs.json`，
预合成音频放在 `media/audio/` 下，路径相对 pack 根目录。