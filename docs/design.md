# EaLearn 架构设计

桌面端语言学习工具，Linux + Windows。核心约束：**程序与语言内容完全解耦**——同一套应用模块，切换语言只换内容包，用户可直接导入第三方包。

## 1. 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 壳 | Tauri 2 | 体积约 10 MB；Linux WebKitGTK / Windows WebView2 |
| 前端 | React + TypeScript + Vite | 文本密集排版、聊天窗富交互，Web 技术成本最低 |
| 内核 | Rust (`crates/core`) | 无 GUI 依赖，可单测，可复用到 CLI |
| 存储 | SQLite (rusqlite) | 进度、SRS、聊天记录、设置 |
| 内容 | 磁盘 pack 目录 | 只读资产，启动校验 hash |

不选 Electron（体积/内存），不选 Flutter（富文本排版与无障碍弱）。

## 2. 仓库结构

```
EaLearn/
├── app/                  React 前端（首页 / 练习 / 情景 / 词库 / 发音）
├── src-tauri/            Tauri 壳 + IPC 命令层（薄，只做转发）
├── crates/
│   ├── core/             领域内核：pack 模型与校验、题型判分、组卷、FSRS 调度
│   ├── lang/             Language Profile trait + 英语 / 韩语实现
│   ├── store/            SQLite 本地库：pack 登记、卡片、作答、设置、聊天记录
│   ├── llm/              OpenAI 兼容流式客户端：SSE、正文流式抽取、标注回贴校验
│   ├── tts/              piper 合成与 wav 缓存（按音色 + 文本 hash 命名）
│   └── forge/            内容 CLI：校验、分词标注、预合成音频、对话草稿生成
├── packs/
│   ├── en/               英语内容包
│   └── ko/               韩语内容包
└── docs/
```

`src-tauri` 不放业务逻辑。所有判断在 `core`，前端只渲染与采集答案。

## 3. 内容包（pack）

```
packs/en/
├── manifest.json         语言、版本、内容 hash、profile 名、TTS 音色
├── words.json            词条
├── sentences.json        拼句素材
├── articles.json         文章 + 题目
├── dialogs.json          剧本对话图
├── media/audio/*.wav     预合成 TTS，按「音色 + 文本」hash 命名
```

### 3.1 统一标注单元：token

英语和韩语必须共用同一套形状，否则「换语言只换包」不成立。

```jsonc
{
  "id": "en.w.00123",
  "surface": "gonna",
  "lemma": "going to",
  "gloss": "将要（口语）",
  "tags": ["slang", "reduction"],
  "tokens": [
    {
      "surface": "gonna",        // 原文片段
      "lemma": "going to",
      "reading": "ˈɡɔnə",        // en: IPA / ko: 罗马字
      "pos": "contraction",
      "gloss": "将要（口语）",
      "span": [0, 5]             // 在 surface 中的字节区间，用于对齐校验
    }
  ],
  "audio": "media/audio/ab12cd.wav"
}
```

- 英语：token = 词
- 韩语：token = 形态素。`밥을` 切成 `밥`(NNG) + `을`(JKO)，各自带 lemma 与罗马字

`span` 是硬性字段：校验器要求 `tokens` 覆盖 `surface` 100% 且不重叠，这是防止 LLM 生成的标注与原文错位的唯一手段。

### 3.2 题型

题型以 `id` 注册，每个题型 = 一份 JSON schema + 一个渲染组件 + 一个判分器。

| id | 说明 | 素材来源 |
|---|---|---|
| `build-sentence` | 打乱词块，用户拼成句子 | sentences.json |
| `word-from-audio` | 听音选词 | words.json + audio |
| `recall` | 背诵卡：正面出词、背面出义，自评熟练度 | words.json 或自定义词条 |
| `article-mcq` | 文章阅读理解 + 选择题 | articles.json |
| `chat-scenario` | 对话框情景演练 | dialogs.json |

判分器写在 `crates/core`（纯函数 `grade(exercise, answer) -> Score`），不在前端。加新题型 = 往 `core` 加判分器 + 往前端加组件，两侧注册表各加一行，不改核心。

## 4. Language Profile

`crates/lang` 定义 trait，每种语言一个实现，由 manifest 的 `"profile"` 字段选择。

```rust
pub trait LanguageProfile {
    fn id(&self) -> &'static str;                              // "en" | "ko"
    fn tokenize(&self, text: &str) -> Vec<Token>;
    fn lemmatize(&self, token: &Token) -> Option<String>;
    fn reading(&self, text: &str) -> Option<String>;           // IPA / 罗马字
    fn tts_voice(&self) -> VoiceSpec;
    fn stopwords(&self) -> &'static [&'static str];
    fn grammar_tags(&self) -> &'static [TagDef];
}
```

- **en**：空白 + 标点切分；词形还原走高頻不规则表 + 规则后缀，不做纯算法猜测
- **ko**：`lindera`（mecab-ko-dic）做形态素分析；罗马字按「국어의 로마자 표기법」

已实测的韩语事实（决定 token 边界怎么定）：

- 名词 + 助词会拆：`밥을` → `밥`(NNG) + `을`(JKO)，`학교에` → `학교`(NNG) + `에`(JKB)
- **动词活用不拆**：`갑니다` 是单个 token，词性标签为复合形式 `VV+EC`。词干要从 ko-dic 的 `expression` 字段取（`가/VV/*+ᄇ니다/EC/*` → `가`），SRS 才能把 `갑니다` / `갔어요` 归到同一个词
- ko-dic 自带 mecab-ko-dic 的左空格惩罚规则（`space_penalty_from_dictionary`），必须开启，否则跨空格的切分会错
- 词性字段名是 `part_of_speech_tag`，不是 `part_of_speech`
- 罗马字实现 연음、ㅎ 脱落与激音化、구개음화、비음화、유음화、ㄹ 鼻音化：`감사합니다` → `gamsahamnida`、`종로` → `jongno`、`신라` → `silla`、`같이` → `gachi`。例外词（ㅎ 弱化、첨가음、特殊活用）仍按拼写转写

### 关键约束

英语与韩语必须共用同一套 token 形状。英语阶段不能走「把词直接当 token」的捷径，否则韩语形态素切分会返工：`밥을` 是 `밥`(NNG) + `을`(JKO)，而英语一个词就是一个 token。两个 pack 现在都走同一个 trait，`forge validate` 用同一条对齐校验兜底。

## 5. 内容生产流水线

内容走「LLM 辅助生成 + 人工校」，全部在仓库里用 CLI 完成，不进应用界面。

```
forge corpus words / sentences → 从 ECDICT、Tatoeba 批量导入词条与例句（筛选、去重、补 token）
forge gen dialog → LLM 出对话草稿，按语言 profile 补 token 后写到 drafts/
forge validate   → schema 校验 + span 覆盖校验 + 素材存在性 + 内容指纹
forge audio      → 批量预合成 wav 并回写 pack，清理无引用文件
forge hash       → 打印内容指纹，写进 manifest 的 content_hash
```

**硬性分界**：应用运行时绝不调用 LLM 生成教学内容。运行时只消费已校验的 pack。LLM 只出现在两处——离线生产（forge），以及情景模式的自由聊天（见 §6）。理由是生成内容的俚语质量与标注准确性必须有人过一道，否则错标注会直接变成肌肉记忆。

人工校对目前是直接改 pack 里的 JSON，再跑 `forge validate` 与 `forge hash`；应用内的校对界面（Studio）还没做。

## 6. 情景模式（聊天窗口）

两条线，共用同一个气泡渲染器（消息 + 可点开的标注层）。

**A. 剧本对话（离线主线）**

`dialogs.json` 是节点图：节点含说话者、消息内容、分支选项。用户回复方式三种：选选项、拖拽词块拼句、自由输入（用 token 对齐做宽松匹配，允许省略部分虚词）。

内容主题覆盖日常：八卦、闲聊、约会、抱怨、砍价。每条消息挂 `annotate` 层，点开看缩写、俚语含义、语法点——这是普通背单词软件做不到的部分。

**B. LLM 自由聊天（用户自备 API key）**

- 流式输出，prompt 注入该语言的 slang registry（取自 pack 的 `tags`），要求返回结构化 JSON：消息正文 + 标注 + 难度
- 每轮把用户输入过一遍 `lemmatize`，找出用户词库里没有的词，标成「可入卡」
- 无 key 或断网时此线不可用，A 线不受影响
- 聊天内容默认不入 SRS，用户显式点击才入卡

协议细节（已实现，见 `crates/llm`）：

- 模型必须只输出一个 JSON 对象，且 `reply` 必须是第一个字段。这样在 JSON 还没闭合时就能把正文逐字抽出来做打字效果，不用等整段生成完
- 客户端只解析 `data:` 帧；SSE 行边界、`[DONE]` 哨兵、多行 data 合并都在 `sse.rs` 里处理，可脱网单测
- 标注做**回贴校验**：`surface` 必须能在 `reply` 原文里找到，找不到整条丢弃并记 warning。模型爱标它没说过的话，这层挡掉
- 空纠正（`fixed == original`）丢弃；`difficulty` 收敛到 1–5
- 供应商差异只在 `base_url`；换成 VPS 上的自建代理同样兼容，客户端无需改动
- API key 目前明文存在本地库的 `settings` 表，仅回显末 4 位；加密留到打包前处理

配置页实现：供应商预设、`base_url`、key 管理（只回显末 4 位）、确认端点后按接口返回的
模型列表做下拉选择。额度显示与总开关未做。

## 7. TTS

接口在 `core` 固定：`tts::synth(text, voice) -> AudioBytes`，引擎实现可换。

选型结论：**只用 piper**。英语 `en_US-lessac`（medium），韩语 `ko_KR-kss`（medium）。传统共振峰合成（espeak 一类）音质差到影响听辨训练，不提供该选项；没有模型时直接报错而不是悄悄退化成机械音。

manifest 的 `voice` 写成 `piper:<模型名>`，音色列表只列本机模型目录里真实存在的模型。

piper 的坑（实测）：`--output_file -` 不写 stdout，会产出空文件，必须写临时文件再读回。

合成器输出统一过一遍 [`normalize_wav`]：RIFF/data 的长度字段若不是实际长度（espeak-ng 就写占位值），GStreamer 会按超长音频处理，WebKitGTK 里表现为点了没声。

- 用户可以在界面里按语言选音色（`tts_voice:<lang>` 存设置）。选的音色与 pack 一致且词条有预生成音频时直接读文件；否则按需合成并缓存到用户数据目录的 `tts/`，缓存键同样是 `sha256(voice + text)`
- 前端只保留一个 `Audio` 元素复用。`new Audio(url).play()` 这种写法对象没有引用，WebKitGTK 会把它回收掉，声音放到一半就断——Linux 下"点了没声/只响一下"多半是这个原因
- 音频文件名 = `sha256(voice + "\0" + text).wav`，落在 pack 的 `media/audio/`。换音色 hash 必变，旧文件不会被误认
- `forge audio <pack>` 批量预合成并写回 pack，已存在的直接复用，`--force` 才重做
- 只有听音在「一到两个词」的词条上出题；整句当四选一答案是送分题（长度一眼看穿）
- 听音题不要求 pack 里已有 wav：缺失时按需合成并缓存到用户数据目录，导入的词条也能直接练听力

策略：发布 pack 前用 `forge audio` 批量预合成并按 hash 存 wav，运行时缺失才补。练的时候不联网。

## 8. 存储

| 表 | 内容 |
|---|---|
| `packs` | 已装包、版本、内容 hash、启用状态 |
| `cards` | SRS 卡：类型（词/句/俚语/语法点）、pack 引用、entry id |
| `reviews` | FSRS 调度状态（稳定性、难度、到期时间） |
| `attempts` | 每次作答：题型、正确、得分率、耗时 |
| `focus_sessions` | 番茄钟完成的专注段 |
| `chat_log` | 自由聊天记录 |
| `custom_entries` | 用户导入的词条（`surface` 在 pack 内唯一，id 由自增列派生，记来源单词书） |
| `custom_articles` | 用户导入的阅读文章：标题、正文、题目 JSON |
| `settings` | UI、TTS 音色、模型与 API key（明文，只回显末 4 位）、学习目标 |

迁移按 `PRAGMA user_version` 逐级补列，不删用户数据。

设置页提供两级清理：`clear_progress` 删卡片/作答/专注/聊天，`clear_all` 再加导入内容与设置；
清完从磁盘重新装配 pack（`build_packs`），语言包在启动时重新登记。

## 9. SRS

FSRS-5 调度。入卡策略：答错必入、点击标注手动入、手动收藏入；答对且没卡只记作答，不把每道题都塞进复习队列。

首页先选练法（混合 / 背单词 / 听力 / 拼句 / 阅读），再组卷。混合模式到期卡先排，剩下的位置用新素材补满，返回的 `review` 就是其中复习题的数量；单项模式只出那一种题，缺素材时首页直接把卡片置灰。
卡种决定题型：句子卡→拼句，词卡→听音→背诵→拼句依次回退。不这么做，复习队列就只是个数字，练的永远是随机新题。

词库页展示的「熟练度」由稳定性换算：`1 - exp(-S/21)`，只用于展示，不参与调度。
没有预生成音频的词条走 `recall`：正面出词（可用运行时 TTS 朗读），用户先自评
认识 / 模糊 / 不认识 / 太简单，选完才显示释义；四个档直接映射成 FSRS 的 1–4 评分。
先出释义再自评会把答案先给用户，评出来的「认识」不可信。

到期卡只按 `kind` 决定题型，不按卡里存的历史——同一张卡下次可能换题型出现，
避免把记忆绑在某一道题的视觉形状上。

### 学习目标

目标存在 `settings`：`goal_kind`（`attempts` / `focus`）、`goal_target`、`goal_book:<lang>`。
首页用当天 `activity` 的作答数或专注分钟数显示进度。设定目标时可以绑定一本词书，
界面按「目标 × 30 天」估算所需词量并推荐一本够用的，保存时若这本书还没导入就自动下载导入。
目标只决定每天练多少、用哪本书，不参与 FSRS 调度。有题量目标时，练习页按
「今天还差多少题」决定这一组的长度（上限 40），练完一组即达标。

## 10. 里程碑

| | 内容 | 状态 |
|---|---|---|
| M0 | Tauri 骨架 + pack 加载 + SQLite | 完成 |
| M1 | 英语 + `build-sentence` + lang trait | 完成 |
| M2 | 四种题型 + TTS 预合成缓存 | 完成，离线可用 |
| M3 | 文章模式 | 完成，正文与题目分开加载 |
| M4 | 剧本情景模式 | 完成，回复走分支选项 |
| M5 | forge 流水线 + Studio | 校验/标注/音频/对话草稿齐全，Studio 未做 |
| M6 | LLM 自由聊天 | 完成，无 key 时降级提示 |
| M7 | 韩语 pack | 完成，应用代码零改动 |
| M8 | 统计、错题本、CSV 导出、在线书目与音色目录 | 完成 |

M7 的出口条件就是整个架构的验收标准：加一门语言只写 pack 与 lang 实现，`app/` 与 `src-tauri/` 零改动。

## 11. 已知最大成本

不是代码，是内容：每个 pack 的词条、句子、对话都要人工校一遍，标注错位会直接变成肌肉记忆。