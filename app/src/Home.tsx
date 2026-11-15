import type { PackSummary, Stats } from "./api";
import { Checkin, Pomodoro } from "./Focus";

export type Mode = "recall" | "audio" | "sentence" | "article" | "mixed";

const MODES: { id: Mode; name: string; desc: string; key: keyof PackSummary["coverage"] | null }[] = [
  { id: "recall", name: "背单词", desc: "正面出词，背面释义，自评熟练度", key: "recall" },
  { id: "audio", name: "听力", desc: "听发音选单词", key: "wordFromAudio" },
  { id: "sentence", name: "拼句子", desc: "拖动词块还原整句", key: "buildSentence" },
  { id: "article", name: "阅读", desc: "读短文做选择题", key: "articleMcq" },
  { id: "mixed", name: "混合复习", desc: "到期卡优先，其余随机", key: null },
];

export function Home({
  lang,
  pack,
  stats,
  onStart,
}: {
  lang: string;
  pack: PackSummary | undefined;
  stats: Stats | null;
  onStart: (mode: Mode) => void;
}) {
  const count = (key: keyof PackSummary["coverage"] | null) => {
    if (!pack) return 0;
    const c = pack.coverage;
    if (!key) return c.recall + c.wordFromAudio + c.buildSentence + c.articleMcq;
    return c[key];
  };

  return (
    <div className="panel home">
      <header className="home-head">
        <div>
          <h2>今天练什么</h2>
          <p className="muted">
            {stats
              ? `待复习 ${stats.due_now} · 今日 ${stats.attempts_today} 题 · 正确率 ${Math.round(
                  stats.accuracy_today * 100,
                )}%`
              : "开始第一组练习吧"}
          </p>
        </div>
      </header>

      <div className="modes">
        {MODES.map((m) => {
          const n = count(m.key);
          const disabled = n === 0;
          return (
            <button
              key={m.id}
              className={`mode-card${disabled ? " disabled" : ""}`}
              disabled={disabled}
              onClick={() => onStart(m.id)}
            >
              <span className="mode-name">{m.name}</span>
              <span className="mode-desc">{m.desc}</span>
              <span className="mode-count">
                {disabled ? "本语言暂无素材" : m.id === "mixed" ? `${n}+ 题可出` : `${n} 个`}
              </span>
            </button>
          );
        })}
      </div>

      <div className="home-bottom">
        <Pomodoro />
        <Checkin />
      </div>

      <p className="muted home-hint">当前语言：{pack?.display ?? lang}</p>
    </div>
  );
}