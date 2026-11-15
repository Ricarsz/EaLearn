// 生成 packs/ko 的初始韩语内容。标注同样由 `forge tokens ko` 产出。
// 用法：node tools/seed_pack_ko.mjs（需先 cargo build -p ealearn-forge）
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const FORGE = join(ROOT, "target/debug/forge");
const PACK = join(ROOT, "packs/ko");

const tok = (text) =>
  JSON.parse(execFileSync(FORGE, ["tokens", "ko", text], { encoding: "utf8" }));

const word = (id, surface, gloss, tags = []) => ({
  id,
  surface,
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
  word("ko.w.0001", "밥", "饭、一餐"),
  word("ko.w.0002", "물", "水"),
  word("ko.w.0003", "친구", "朋友"),
  word("ko.w.0004", "약속", "约定、约好的事"),
  word("ko.w.0005", "짜증나다", "烦、来气"),
  word("ko.w.0006", "눈치", "眼力见，会看气氛", ["slang"]),
  word("ko.w.0007", "헐", "哇（无语、震惊时用）", ["slang"]),
  word("ko.w.0008", "대박", "太棒了、太厉害了", ["slang"]),
  word("ko.w.0009", "짝사랑", "单恋"),
  word("ko.w.0010", "밀당", "恋爱里的推拉、欲擒故纵", ["slang"]),
];

const sentences = [
  sentence("ko.s.0001", "오늘 진짜 짜증나", "今天真烦"),
  sentence("ko.s.0002", "내일 친구랑 약속 있어", "明天跟朋友有约"),
  sentence("ko.s.0003", "걔가 또 약속을 깼어", "他又放鸽子了", ["slang"]),
  sentence("ko.s.0004", "너 완전 대박이다", "你太厉害了", ["slang"]),
];

const articleBody =
  "약속을 잘 지키는 사람은 신뢰를 얻는다. 반대로 자주 약속을 어기면 친구를 잃는다. 시간을 지키는 것은 작은 배려다.";

const articles = [
  {
    id: "ko.a.0001",
    title: "약속을 지키는 사람",
    body: articleBody,
    tokens: tok(articleBody),
    tags: ["everyday"],
    difficulty: 2,
    questions: [
      {
        prompt: "文章说，守约的人能得到什么？",
        options: ["신뢰（信任）", "돈（钱）", "시간（时间）"],
        answer: 0,
        explain: "原文：신뢰를 얻는다。",
      },
      {
        prompt: "文中提到的「작은 배려（小小的体贴）」指什么？",
        options: ["시간을 지키는 것（守时）", "선물을 주는 것（送礼物）", "말을 많이 하는 것（多说话）"],
        answer: 0,
        explain: "原文：시간을 지키는 것은 작은 배려다。",
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
    id: "ko.d.0001",
    title: "约会放鸽子",
    theme: "gossip",
    start: "n1",
    tags: ["slang", "dating"],
    nodes: [
      node("n1", "a", "너 어제 봤어? 걔가 또 약속을 깼어.", { next: "n2" }),
      node("n2", "narrator", "", {
        choices: [
          { text: "헐, 진짜?", next: "n3" },
          { text: "누구?", next: "n5" },
          { text: "별로 관심 없어.", next: "n6", awkward: true },
        ],
      }),
      node("n3", "a", "진짜야. 나 완전 짜증나.", { next: "n4" }),
      node("n4", "narrator", "", {
        choices: [
          { text: "그래도 너무 신경 쓰지 마.", next: "n6" },
          { text: "다음엔 나도 같이 가.", next: "n6" },
        ],
      }),
      node("n5", "a", "민수야. 어제 또 그랬어.", { next: "n3" }),
      node("n6", "a", "고마워. 너는 참 좋은 친구야."),
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
const base = {
  schema: 1,
  lang: "ko",
  display: "韩语",
  version: "0.1.0",
  profile: "ko",
  voice: "espeak:ko",
};
writeFileSync(manifestPath, JSON.stringify(base, null, 2) + "\n");

const hash = execFileSync(FORGE, ["hash", PACK], { encoding: "utf8" }).trim();
writeFileSync(
  manifestPath,
  JSON.stringify({ ...base, content_hash: hash }, null, 2) + "\n",
);

console.log(
  `packs/ko: ${words.length} 词 / ${sentences.length} 句 / ${articles.length} 篇 / ${dialogs.length} 段对话 · hash ${hash.slice(0, 12)}…`,
);