import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type CardItem } from "./api";
import { playUrl } from "./audio";

type Filter = "all" | "due" | "weak" | "custom";

type ParsedRow = { surface: string; gloss?: string; tags?: string; line: number };

/// 与 Rust 侧 `ealearn_core::import::parse` 同一套规则，用来做导入前预览。
function parseRows(text: string): ParsedRow[] {
  const rows: ParsedRow[] = [];
  const seen = new Set<string>();
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const fields = (line.includes("\t") ? line.split("\t") : line.split(",")).map((f) => f.trim());
    const surface = fields[0];
    if (!surface) return;
    const key = surface.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    rows.push({
      surface,
      gloss: fields[1] || undefined,
      tags: fields[2] || undefined,
      line: i + 1,
    });
  });
  return rows;
}

export function Wordbook({ lang, onPractice }: { lang: string; onPractice: () => void }) {
  const [rows, setRows] = useState<CardItem[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [importing, setImporting] = useState(false);
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [skipped, setSkipped] = useState<string[]>([]);
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
  const customCount = (rows ?? []).filter((r) => r.is_custom).length;
  const dueCount = (rows ?? []).filter((r) => r.due <= now).length;
  const weakCount = (rows ?? []).filter((r) => r.mastery < 0.4).length;

  const visible = (rows ?? []).filter((r) => {
    if (filter === "due") return r.due <= now;
    if (filter === "weak") return r.mastery < 0.4;
    if (filter === "custom") return r.is_custom;
    return true;
  });

  const parsed = useMemo(() => parseRows(draft), [draft]);

  const doImport = async () => {
    setError(null);
    setSkipped([]);
    try {
      const r = await api.importWords(lang, draft);
      setNote(
        `新增 ${r.added} 个` + (r.updated ? `，更新 ${r.updated} 个` : "") +
          (r.skipped.length ? `，跳过 ${r.skipped.length} 行` : ""),
      );
      setSkipped(r.skipped);
      setDraft("");
      setImporting(false);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const removeCard = async (id: number) => {
    try {
      await api.deleteCard(id);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const removeCustom = async (entryId: string, cardId: number) => {
    try {
      await api.deleteCustomEntry(lang, entryId);
      await api.deleteCard(cardId).catch(() => {});
      await load();
      setNote("已删除自定义词条");
    } catch (e) {
      setError(String(e));
    }
  };

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
            不熟 {weakCount}
          </button>
          <button className={filter === "custom" ? "on" : ""} onClick={() => setFilter("custom")}>
            自定义 {customCount}
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
            每行一个：<code>单词</code> 制表符 <code>释义</code> 制表符 <code>标签</code>，
            后两列可省。没有制表符时按逗号切。以 <code>#</code> 开头的行与空行忽略，重复的词只取第一次。
          </p>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={"ghost\t玩消失，失联\tslang\nspill the tea\t说八卦\nvibe"}
          />
          <div className="import-file">
            <label className="file-btn">
              从文件读取
              <input
                type="file"
                accept=".txt,.tsv,.csv"
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (f) setDraft(await f.text());
                  e.target.value = "";
                }}
              />
            </label>
            <span className="muted">
              {parsed.length ? `${parsed.length} 个词待导入` : "还没有内容"}
            </span>
            <div className="spacer" />
            <button className="primary" disabled={parsed.length === 0} onClick={doImport}>
              导入 {parsed.length ? `(${parsed.length})` : ""}
            </button>
          </div>

          {parsed.length > 0 && (
            <ul className="preview">
              {parsed.slice(0, 8).map((r) => (
                <li key={r.surface}>
                  <b>{r.surface}</b>
                  <span className="muted">{r.gloss ?? "（无释义）"}</span>
                  {r.tags && <span className="tag">{r.tags}</span>}
                </li>
              ))}
              {parsed.length > 8 && <li className="muted">…还有 {parsed.length - 8} 个</li>}
            </ul>
          )}
        </div>
      )}

      {note && <p className="ok-line">{note}</p>}
      {skipped.length > 0 && (
        <ul className="skipped">
          {skipped.slice(0, 6).map((s, i) => (
            <li key={i} className="muted">{s}</li>
          ))}
        </ul>
      )}
      {error && <p className="error">{error}</p>}
      {rows === null && <p className="muted">读取中…</p>}
      {rows?.length === 0 && (
        <p className="muted">还没有词卡。答错会自动入卡，也可以导入自己的词。</p>
      )}

      <ul className="card-list">
        {visible.map((r) => (
          <li key={r.card_id} className="card-row">
            <div className="card-main">
              <div className="card-title">
                <b>{r.surface}</b>
                <button
                  className="play small-play"
                  title="发音"
                  onClick={() =>
                    api.speak(lang, r.surface, r.entry_id).then(playUrl).catch(() => {})
                  }
                >
                  ▶
                </button>
                {r.is_custom && <span className="tag">自定义</span>}
              </div>
              <div className="muted">{r.gloss ?? "—"}</div>
            </div>

            <div
              className="mastery"
              title={`稳定性 ${r.stability.toFixed(1)} 天 · 复习 ${r.reps} 次 · 遗忘 ${r.lapses} 次`}
            >
              <div className="mastery-bar">
                <div className="mastery-fill" style={{ width: `${Math.round(r.mastery * 100)}%` }} />
              </div>
              <span className="muted">{Math.round(r.mastery * 100)}%</span>
            </div>

            <div className="due muted">{dueLabel(r.due, now)}</div>

            {r.is_custom ? (
              <button
                className="ghost"
                title="删除自定义词条"
                onClick={() => removeCustom(r.entry_id, r.card_id)}
              >
                ✕
              </button>
            ) : (
              <button className="ghost" title="移出复习" onClick={() => removeCard(r.card_id)}>
                ✕
              </button>
            )}
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