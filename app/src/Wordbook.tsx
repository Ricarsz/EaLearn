import { useCallback, useEffect, useState } from "react";
import { api, type CardItem } from "./api";
import { playUrl } from "./audio";

type Filter = "all" | "due" | "weak";

export function Wordbook({ lang, onPractice }: { lang: string; onPractice: () => void }) {
  const [rows, setRows] = useState<CardItem[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [importing, setImporting] = useState(false);
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRows(await api.listCards(lang));
    } catch (e) {
      setError(String(e));
    }
  }, [lang]);

  useEffect(() => {
    load();
  }, [load]);

  const now = Math.floor(Date.now() / 1000);
  const visible = (rows ?? []).filter((r) => {
    if (filter === "due") return r.due <= now;
    if (filter === "weak") return r.mastery < 0.4;
    return true;
  });

  const doImport = async () => {
    setError(null);
    try {
      const r = await api.importWords(lang, draft);
      setNote(
        `导入 ${r.added} 个词` + (r.skipped.length ? `，跳过 ${r.skipped.length} 行` : ""),
      );
      setDraft("");
      setImporting(false);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (id: number) => {
    try {
      await api.deleteCard(id);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const dueCount = (rows ?? []).filter((r) => r.due <= now).length;

  return (
    <div className="panel wide">
      <div className="wordbook-head">
        <div className="filters">
          <button className={filter === "all" ? "on" : ""} onClick={() => setFilter("all")}>
            全部 {rows?.length ?? 0}
          </button>
          <button className={filter === "due" ? "on" : ""} onClick={() => setFilter("due")}>
            待复习 {dueCount}
          </button>
          <button className={filter === "weak" ? "on" : ""} onClick={() => setFilter("weak")}>
            不熟 {(rows ?? []).filter((r) => r.mastery < 0.4).length}
          </button>
        </div>
        <div className="actions">
          <button onClick={() => setImporting((v) => !v)}>导入单词</button>
          <button className="primary" disabled={dueCount === 0} onClick={onPractice}>
            开始复习
          </button>
        </div>
      </div>

      {importing && (
        <div className="import-box">
          <p className="muted">
            每行一个：<code>单词</code> + 制表符 + 释义，第三列可选标签。也可以用逗号分隔。
          </p>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={"ghost\t玩消失，失联\tslang\nspill the tea\t说八卦"}
          />
          <div className="actions">
            <button className="primary" disabled={!draft.trim()} onClick={doImport}>
              导入
            </button>
          </div>
        </div>
      )}

      {note && <p className="muted">{note}</p>}
      {error && <p className="error">{error}</p>}

      {rows === null && <p className="muted">读取中…</p>}
      {rows?.length === 0 && <p className="muted">还没有词卡。答错会自动入卡，也可以导入自己的词。</p>}

      <ul className="card-list">
        {visible.map((r) => (
          <li key={r.card_id} className="card-row">
            <div className="card-main">
              <div className="card-title">
                <b>{r.surface}</b>
                <button
                  className="play small-play"
                  onClick={() => api.speak(lang, r.surface, r.entry_id).then(playUrl).catch(() => {})}
                >
                  ▶
                </button>
                {r.is_custom && <span className="tag">自定义</span>}
              </div>
              <div className="muted">{r.gloss ?? "—"}</div>
            </div>

            <div className="mastery" title={`稳定性 ${r.stability.toFixed(1)} 天 · 复习 ${r.reps} 次`}>
              <div className="mastery-bar">
                <div className="mastery-fill" style={{ width: `${Math.round(r.mastery * 100)}%` }} />
              </div>
              <span className="muted">{Math.round(r.mastery * 100)}%</span>
            </div>

            <div className="due muted">{dueLabel(r.due, now)}</div>

            <button className="ghost" title="移出复习" onClick={() => remove(r.card_id)}>
              ✕
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function dueLabel(due: number, now: number) {
  if (due <= now) return "待复习";
  const days = Math.round((due - now) / 86_400);
  if (days <= 1) return "明天";
  return `${days} 天后`;
}
