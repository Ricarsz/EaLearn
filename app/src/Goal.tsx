import { useEffect, useMemo, useState } from "react";
import { api, type RemoteBook } from "./api";

export type GoalKind = "attempts" | "focus";

export type Goal = {
  kind: GoalKind;
  target: number;
  /** 选中的词书（可选），保存目标时自动导入 */
  bookId: string;
  bookName: string;
};

export const GOAL_KIND = "goal_kind";
export const GOAL_TARGET = "goal_target";
const bookKey = (lang: string) => `goal_book:${lang}`;
const bookNameKey = (lang: string) => `goal_book_name:${lang}`;

export async function loadGoal(lang: string): Promise<Goal | null> {
  const [kind, target, bookId, bookName] = await Promise.all([
    api.getSetting(GOAL_KIND).catch(() => null),
    api.getSetting(GOAL_TARGET).catch(() => null),
    api.getSetting(bookKey(lang)).catch(() => null),
    api.getSetting(bookNameKey(lang)).catch(() => null),
  ]);
  const n = Number(target ?? "");
  if (!kind || !Number.isFinite(n) || n <= 0) return null;
  return {
    kind: kind === "focus" ? "focus" : "attempts",
    target: Math.round(n),
    bookId: bookId ?? "",
    bookName: bookName ?? "",
  };
}

export function goalUnit(kind: GoalKind) {
  return kind === "focus" ? "分钟" : "题";
}

export function GoalCard({
  goal,
  done,
  onEdit,
}: {
  goal: Goal | null;
  done: number;
  onEdit: () => void;
}) {
  if (!goal) {
    return (
      <section className="goal-card empty">
        <div>
          <p className="section-label">今日目标</p>
          <p className="muted">定一个每天的量：做多少题，或专注多久。选词书会自动下载导入。</p>
        </div>
        <button className="small" onClick={onEdit}>
          设定目标
        </button>
      </section>
    );
  }

  const unit = goalUnit(goal.kind);
  const ratio = Math.min(1, goal.target > 0 ? done / goal.target : 0);
  const left = Math.max(0, goal.target - done);

  return (
    <section className="goal-card">
      <div className="goal-head">
        <p className="section-label">今日目标</p>
        <button className="ghost small" onClick={onEdit}>
          调整
        </button>
      </div>
      <div className="goal-line">
        <span className="goal-numbers">
          <b>{done}</b> / {goal.target} {unit}
        </span>
        <span className="muted">{left === 0 ? "今天的量做完了" : `还差 ${left} ${unit}`}</span>
      </div>
      <div className="goal-bar">
        <div className="goal-fill" style={{ width: `${Math.round(ratio * 100)}%` }} />
      </div>
      {goal.bookName && <p className="muted">词书：{goal.bookName}</p>}
    </section>
  );
}

/// 目标设置。词书按「目标 × 30 天」推荐一本够用的，保存时自动导入。
export function GoalDialog({
  lang,
  goal,
  onClose,
  onSaved,
}: {
  lang: string;
  goal: Goal | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [kind, setKind] = useState<GoalKind>(goal?.kind ?? "attempts");
  const [target, setTarget] = useState(goal?.target ?? 20);
  const [bookId, setBookId] = useState(goal?.bookId ?? "");
  const [books, setBooks] = useState<RemoteBook[]>([]);
  const [touchedBook, setTouchedBook] = useState(!!goal?.bookId);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .remoteBooks()
      .then((list) =>
        alive && setBooks(list.filter((b) => b.lang === lang && (b.kind ?? "words") === "words")),
      )
      .catch((e) => alive && setError(`读取词书索引失败：${e}`));
    return () => {
      alive = false;
    };
  }, [lang]);

  // 按 30 天估算需要多少词，推荐一本刚好够的书
  const need = kind === "attempts" ? Math.max(1, target) * 30 : 0;
  const recommended = useMemo(() => {
    if (!books.length || need <= 0) return null;
    const enough = books.filter((b) => b.count >= need).sort((a, b) => a.count - b.count);
    return enough[0] ?? books.slice().sort((a, b) => b.count - a.count)[0] ?? null;
  }, [books, need]);

  useEffect(() => {
    if (touchedBook) return;
    if (recommended) setBookId(recommended.id);
  }, [recommended, touchedBook]);

  const selected = books.find((b) => b.id === bookId) ?? null;

  const chips = kind === "attempts" ? [10, 20, 30, 50] : [15, 25, 45, 60];

  const save = async () => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await api.setSetting(GOAL_KIND, kind);
      await api.setSetting(GOAL_TARGET, String(target));
      const book = books.find((b) => b.id === bookId) ?? null;
      await api.setSetting(bookKey(lang), book?.id ?? "");
      await api.setSetting(bookNameKey(lang), book?.name ?? "");

      if (book && book.imported === 0) {
        setNote(`正在导入《${book.name}》…`);
        const r = await api.installRemoteBook(lang, book.name, book.url);
        setNote(`已导入《${book.name}》：新增 ${r.added} 个，更新 ${r.updated} 个`);
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(`目标已保存，但词书导入失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-head">
          <h2>学习目标</h2>
          <button className="ghost" onClick={onClose}>
            关闭
          </button>
        </header>

        <section>
          <p className="section-label">每天做什么</p>
          <div className="phase-switch">
            <button
              className={kind === "attempts" ? "on" : ""}
              onClick={() => setKind("attempts")}
            >
              做题数
            </button>
            <button className={kind === "focus" ? "on" : ""} onClick={() => setKind("focus")}>
              专注时长
            </button>
          </div>

          <label>
            每天 {kind === "attempts" ? "做几道题" : "专注多少分钟"}
            <input
              type="number"
              min={1}
              max={999}
              value={target}
              onChange={(e) => setTarget(Math.max(1, Math.min(999, Number(e.target.value) || 0)))}
            />
          </label>
          <div className="chips">
            {chips.map((n) => (
              <button
                key={n}
                className={`chip-btn${target === n ? " on" : ""}`}
                onClick={() => setTarget(n)}
              >
                {n} {kind === "attempts" ? "题" : "分钟"}
              </button>
            ))}
          </div>
        </section>

        <section>
          <p className="section-label">跟哪本词书（可选）</p>
          {books.length === 0 && <p className="muted">这门语言还没有可导入的词书。</p>}
          {books.length > 0 && (
            <>
              <label>
                词书
                <select
                  value={bookId}
                  onChange={(e) => {
                    setTouchedBook(true);
                    setBookId(e.target.value);
                  }}
                >
                  <option value="">不绑定词书</option>
                  {books.map((b) => (
                    <option key={b.id} value={b.id}>
                      {b.name} · {b.count} 词{b.imported > 0 ? " · 已下载" : ""}
                    </option>
                  ))}
                </select>
              </label>
              {kind === "attempts" && (
                <p className="muted">
                  按 30 天估算需要约 {need} 词
                  {recommended ? `，推荐《${recommended.name}》` : ""}。
                </p>
              )}
              {selected && selected.imported === 0 && (
                <p className="muted">保存时会自动下载并导入这本词书。</p>
              )}
            </>
          )}
        </section>

        {note && <p className="ok-line">{note}</p>}
        {error && <p className="error">{error}</p>}

        <div className="actions">
          <button className="primary" onClick={save} disabled={busy}>
            {busy ? "保存中…" : "保存目标"}
          </button>
        </div>
      </div>
    </div>
  );
}