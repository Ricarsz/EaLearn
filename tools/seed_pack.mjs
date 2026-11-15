// 一次性种子脚本：首次生成 packs/en。内容改动后重跑即可，span 由 forge 重新产出。
// 用法：node tools/seed_pack.mjs（需先 cargo build -p ealearn-forge）。
// 标注（span/lemma）全部由 `forge tokens` 产出，脚本里不手写 span。
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const FORGE = join(ROOT, "target/debug/forge");
const PACK = join(ROOT, "packs/en");

const tok = (text) =>
  JSON.parse(execFileSync(FORGE, ["tokens", "en", text], { encoding: "utf8" }));

const word = (id, surface, gloss, tags = [], lemma) => ({
  id,
  surface,
  ...(lemma ? { lemma } : {}),
  gloss,
  tags,
  tokens: tok(surface),
});

const sentence = (id, surface, gloss, tags = []) => ({
  id,
  surface,
  gloss,
  tags,
  tokens: tok(surface),
});

const words = [
  word("en.w.0001", "gonna", "将要，准备（口语缩略）", ["slang", "reduction"], "going to"),
  word("en.w.0002", "bail", "放鸽子；中途退出", ["slang"]),
  word("en.w.0003", "lowkey", "偷偷地；有点（口语）", ["slang"]),
  word("en.w.0004", "ghost", "突然失联、玩消失（恋爱语境）", ["slang"]),
  word("en.w.0005", "crush", "暗恋对象", ["everyday"]),
  word("en.w.0006", "vibe", "氛围、感觉", ["slang"]),
  word("en.w.0007", "hang out", "一起玩、闲晃", ["phrasal"]),
  word("en.w.0008", "spill the tea", "说八卦、爆料", ["slang", "idiom"]),
];

const sentences = [
  sentence("en.s.0001", "I kind of want to bail on the party", "我有点不想去那个派对了"),
  sentence("en.s.0002", "She totally ghosted me after the date", "约会之后她直接玩消失了"),
  sentence("en.s.0003", "Let's hang out this weekend if you're free", "你周末有空的话一起出去玩吧"),
  sentence("en.s.0004", "He lowkey has a crush on you", "他其实偷偷暗恋你"),
  sentence("en.s.0005", "Spill the tea, what happened last night", "快说八卦，昨晚到底怎么了", ["slang"]),
];

const articleBody =
  "Ghosting used to be rude. Now it is normal. People stop replying because a real conversation feels too heavy. But the person on the other side never learns what went wrong.";

const articles = [
  {
    id: "en.a.0001",
    title: "Why We Ghost",
    body: articleBody,
    tokens: tok(articleBody),
    tags: ["everyday", "dating"],
    difficulty: 2,
    questions: [
      {
        prompt: "文章说，人们现在不回复消息的主要原因是什么？",
        options: [
          "真正的对话让人觉得太沉重",
          "手机没电了",
          "大家都变得更有礼貌",
        ],
        answer: 0,
        explain: "原文：a real conversation feels too heavy。",
      },
      {
        prompt: "被冷落的那个人始终不知道什么？",
        options: ["对方为什么消失", "该几点回消息", "约会去哪家店"],
        answer: 0,
        explain: "原文：never learns what went wrong。",
      },
    ],
  },
];

const node = (id, speaker, text, extra = {}) => ({
  id,
  speaker,
  text,
  ...(text ? { tokens: tok(text) } : {}),
  ...extra,
});

const dialogs = [
  {
    id: "en.d.0001",
    title: "八卦时间",
    theme: "gossip",
    start: "n1",
    tags: ["slang", "everyday"],
    nodes: [
      node("n1", "a", "Did you hear about Mia and Jake?", { next: "n2" }),
      node("n2", "narrator", "", {
        choices: [
          { text: "No way, spill the tea!", next: "n3" },
          { text: "Who?", next: "n5" },
          { text: "Not really my business.", next: "n6", awkward: true },
        ],
      }),
      node("n3", "a", "They broke up last week. He totally ghosted her.", { next: "n4" }),
      node("n4", "narrator", "", {
        choices: [
          { text: "That is so lowkey cruel.", next: "n6" },
          { text: "I kind of saw it coming.", next: "n6" },
        ],
      }),
      node("n5", "a", "Mia's boyfriend. Anyway, they broke up.", { next: "n3" }),
      node("n6", "a", "Right? Anyway, do not tell anyone."),
    ],
  },
];

mkdirSync(join(PACK, "media/audio"), { recursive: true });
mkdirSync(join(PACK, "media/img"), { recursive: true });
writeFileSync(join(PACK, "words.json"), JSON.stringify(words, null, 2) + "\n");
writeFileSync(join(PACK, "sentences.json"), JSON.stringify(sentences, null, 2) + "\n");
writeFileSync(join(PACK, "articles.json"), JSON.stringify(articles, null, 2) + "\n");
writeFileSync(join(PACK, "dialogs.json"), JSON.stringify(dialogs, null, 2) + "\n");

const manifestPath = join(PACK, "manifest.json");
writeFileSync(
  manifestPath,
  JSON.stringify(
    {
      schema: 1,
      lang: "en",
      display: "英语",
      version: "0.1.0",
      profile: "en",
      voice: "en_US-neutral",
    },
    null,
    2,
  ) + "\n",
);

// 内容指纹：写完数据文件后再算，manifest 自身不参与
const hash = execFileSync(FORGE, ["hash", PACK], { encoding: "utf8" }).trim();
writeFileSync(
  manifestPath,
  JSON.stringify(
    {
      schema: 1,
      lang: "en",
      display: "英语",
      version: "0.1.0",
      profile: "en",
      voice: "en_US-neutral",
      content_hash: hash,
    },
    null,
    2,
  ) + "\n",
);

console.log(
  `packs/en: ${words.length} 词 / ${sentences.length} 句 / ${articles.length} 篇 / ${dialogs.length} 段对话 · hash ${hash.slice(0, 12)}…`,
);