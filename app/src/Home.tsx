import { useCallback, useEffect, useState } from "react";
import { api, type DayActivity, type PackSummary, type Stats } from "./api";
import { Checkin, Pomodoro } from "./Focus";
import { GoalCard, GoalDialog, loadGoal, type Goal } from "./Goal";

export type Mode = "recall" | "audio" | "sentence" | "article" | "mixed" | "weak";

/// 单项练法。混合复习不在这里——它是首页的主按钮。
const MODES: { id: Mode; name: string; desc: string; key: keyof PackSummary["coverage"] }[] = [
  { id: "recall", name: "背单词", desc: "正面出词，背面释义，自评熟练度", key: "recall" },
  { id: "audio", name: "听力", desc: "听发音选单词", key: "wordFromAudio" },
  { id: "sentence", name: "拼句子", desc: "拖动词块还原整句", key: "buildSentence" },
  { id: "article", name: "阅读", desc: "读短文做选择题", key: "articleMcq" },
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
  const c = pack?.coverage;
  const total = c ? c.recall + c.wordFromAudio + c.buildSentence + c.articleMcq : 0;
  const due = stats?.due_now ?? 0;
  const nothing = total === 0 && due === 0;

  const [goal, setGoal] = useState<Goal | null>(null);
  const [today, setToday] = useState<DayActivity | null>(null);
  const [editing, setEditing] = useState(false);

  const refresh = useCallback(() => {
    loadGoal(lang)
      .then(setGoal)
      .catch(() => setGoal(null));
    api
      .activity(1, -new Date().getTimezoneOffset())
      .then((days) => setToday(days[0] ?? null))
      .catch(() => {});
  }, [lang]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const done = goal ? (goal.kind === "focus" ? (today?.focus_minutes ?? 0) : (today?.attempts ?? 0)) : 0;

  return (
    <div className="panel home">
      <header className="home-head">
        <div>
          <h2>今天练什么</h2>
          <p className="muted">
            {stats
              ? `待复习 ${due} · 今日 ${stats.attempts_today} 题 · 正确率 ${Math.round(
                  stats.accuracy_today * 100,
                )}%`
              : "开始第一组练习吧"}
          </p>
        </div>
      </header>

      <GoalCard goal={goal} done={done} onEdit={() => setEditing(true)} />

      <button
        className="start-mixed"
        disabled={nothing}
        onClick={() => onStart("mixed")}
      >
        <span className="start-main">
          <span className="start-label">混合复习</span>
          <span className="start-desc">到期卡先排，其余随机补足；答错自动入卡</span>
        </span>
        <span className="start-side">
          {due > 0 ? `待复习 ${due}` : `${total} 题可出`}
        </span>
      </button>

      <div className="modes">
        {MODES.map((m) => {
          const n = c ? c[m.key] : 0;
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
              <span className="mode-count">{disabled ? "本语言暂无素材" : `${n} 个`}</span>
            </button>
          );
        })}
      </div>

      <div className="home-bottom">
        <Pomodoro />
        <Checkin />
      </div>

      <p className="muted home-hint">当前语言：{pack?.display ?? lang}</p>

      {editing && (
        <GoalDialog
          lang={lang}
          goal={goal}
          onClose={() => setEditing(false)}
          onSaved={refresh}
        />
      )}
    </div>
  );
}