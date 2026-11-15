# EaLearn 架构设计

桌面端语言学习工具，Linux + Windows。核心约束：**程序与语言内容完全解耦**——同一套应用模块，切换语言只换内容包，用户可直接导入第三方包。

## 1. 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 壳 | Tauri 2 | 体积约 10 MB；Linux WebKitGTK / Windows WebView2 |
| 前端 | React + TypeScript + Vite | 文本密集排版、聊天窗富交互，Web 技术成本最低 |
| 内核 | Rust (`crates/core`) | 无 GUI 依赖，可单测，可复用到 CLI |
| 存储 | SQLite (sqlx) | 进度、SRS、聊天记录 |
| 内容 | 磁盘 pack 目录 | 只读资产，启动校验 hash |

不选 Electron（体积/内存），不选 Flutter（富文本排版与无障碍弱）。

## 2. 仓库结构

```
EaLearn/
├── app/                  React 前端
├── src-tauri/            Tauri 壳 + IPC 命令层（薄，只做转发）
├── crates/
│   ├── core/             领域内核：题型判分、会话状态机、SRS、pack 校验
│   ├── lang/             Language Profile trait + 各语言实现
│   └── forge/            CLI：LLM 生成 → 校验 → 打包
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
├── media/audio/*.wav     预合成 TTS，按文本 hash 命名
└── media/img/*.webp      看图选词图集
```

### 3.1 统一标注单元：token

英语和韩语必须共用同一套形状，否则「换语言只换包」不成立。

```jsonc
{
  "id": "en.w.00123",
  "lang": "en",
  "surface": "gonna",
  "lemma": "going to",
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
  "media": { "audio": "media/audio/ab12cd.wav", "image": "media/img/9f31.webp" }
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
| `word-from-image` | 看图选词 | words.json + image |
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
- 罗马字目前实现了 연음 与 ㅎ 脱落，未实现 비음화 / 유음화 / 구개음화：`감사합니다` 会得到 `gamsahapnida` 而不是 `gamsahamnida`

### 关键风险

韩语要到最后一个里程碑才进场，如果英语阶段走了捷径（比如把词直接当 token），schema 会在韩语阶段返工。

**缓解**：M1 就让英语也走 trait，不留英语特例；并在 M1 阶段用一个一次性 spike 跑通 `lindera` 切一句韩语，确认 token 边界模型成立后再往下做。这半天成本能省掉后面整轮重构。

## 5. 内容生产流水线

内容和图集走「LLM 辅助生成 + 人工校」。

```
forge gen      → LLM 按严格 JSON schema 出草稿（失败重试 N 次）
forge validate → schema 校验 + span 覆盖校验 + 资产 hash 存在性校验
Studio         → GUI 人工校（逐条确认/改标注，状态 draft → reviewed）
forge pack     → 打压缩包，写版本号与内容 hash
```

**硬性分界**：应用运行时绝不调用 LLM 生成教学内容。运行时只消费已 reviewed 的 pack。LLM 只出现在两处——离线生产（forge），以及情景模式的自由聊天（见 §6）。理由是生成内容的俚语质量与标注准确性必须有人过一道，否则错标注会直接变成肌肉记忆。

Studio 是应用内的隐藏路由，`--studio` 启动才出现，随主程序发布不影响普通用户。

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

配置页需要：key 管理、模型选择、额度显示、关闭开关。

## 7. TTS

接口在 `core` 固定：`tts::synth(text, voice) -> AudioBytes`，引擎实现可换。

选型现状：`piper` 已接入，英语用 `en_US-amy`（medium，61 MB），音质可接受；`espeak-ng` 留作没有模型时的兜底。manifest 的 `voice` 写成 `piper:<模型名>` 或 `espeak:<语言>` 即可切换，同一个前缀决定引擎，换音色不换代码。

piper 的坑（实测）：`--output_file -` 不写 stdout，会产出空文件，必须写临时文件再读回。piper 有韩语音色 `ko_KR-kss-medium`，之前的「韩语得换引擎」判断不成立。

- 用户可以在界面里按语言选音色（`tts_voice:<lang>` 存设置）。选的音色与 pack 一致且词条有预生成音频时直接读文件；否则按需合成并缓存到用户数据目录的 `tts/`，缓存键同样是 `sha256(voice + text)`
- 前端只保留一个 `Audio` 元素复用。`new Audio(url).play()` 这种写法对象没有引用，WebKitGTK 会把它回收掉，声音放到一半就断——Linux 下"点了没声/只响一下"多半是这个原因
- 音频文件名 = `sha256(voice + "\0" + text).wav`，落在 pack 的 `media/audio/`。换音色 hash 必变，旧文件不会被误认
- `forge audio <pack>` 批量预合成并写回 pack，已存在的直接复用，`--force` 才重做
- 看图/听音只在「一到两个词」的词条上出题；整句当四选一答案是送分题（长度一眼看穿）

策略：pack 安装时批量预合成并按文本 hash 存 wav，运行时缺失才补。练的时候不联网。

## 8. 存储

| 表 | 内容 |
|---|---|
| `packs` | 已装包、版本、内容 hash、启用状态 |
| `cards` | SRS 卡：类型（词/句/俚语/语法点）、pack 引用、entry id |
| `reviews` | FSRS 调度状态 |
| `attempts` | 每次作答：题型、正确、耗时 |
| `chat_log` | 自由聊天记录 |
| `custom_entries` | 用户导入的词条（`surface` 在 pack 内唯一，id 由自增列派生） |
| `settings` | UI 语言、TTS 音色、API key（加密） |

## 9. SRS

FSRS-5 调度。入卡策略：答错必入、点击标注手动入、手动收藏入。

组卷时到期卡先排，再用新内容补满；卡种决定题型（句子卡→拼句，词卡→听音→看图→拼句依次回退）。
不这么做，复习队列就只是个数字，练的永远是随机新题。

词库页展示的「熟练度」由稳定性换算：`1 - exp(-S/21)`，只用于展示，不参与调度。
自定义词条没有音频/图片，练习时走 `recall`：正面出词（可用运行时 TTS 朗读）、
背面出义，用户自评 不认识/模糊/认识/太简单，直接映射成 FSRS 的 1–4 评分。

到期卡只按 `kind` 决定题型，不按卡里存的历史——同一张卡下次可能换题型出现，
避免把记忆绑在某一道题的视觉形状上。

## 10. 里程碑

| | 内容 | 出口条件 |
|---|---|---|
| M0 | Tauri 骨架 + pack 加载 + SQLite | 空 pack 能启动 |
| M1 | 英语 + `build-sentence` + lang trait | 拼句能判分；lindera 韩语 spike 通过 |
| M2 | 四种题型 + TTS 预合成缓存 | 离线断网可用 |
| M3 | 文章模式（复用 annotate 层） | 点词可查可入卡 |
| M4 | 剧本情景模式 | 三种回复方式可用 |
| M5 | forge 流水线 + Studio | 新手能产出一个 reviewed pack |
| M6 | LLM 自由聊天 | 无 key 时优雅降级 |
| M7 | 韩语 pack | **不改动任何应用代码**即跑通全流程 |

M7 的出口条件就是整个架构的验收标准：加一门语言只写 pack 与 lang 实现，`app/` 与 `src-tauri/` 零改动。

## 11. 已知最大成本

不是代码，是图集。看图选词需要授权干净的图片，来源（AI 生成 / 开放图集）需要在 M2 之前定下来。