import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type BookInfo, type CardItem, type RemoteBook } from "./api";
import { playUrl } from "./audio";

type Filter = "all" | "due" | "weak" | "custom";

/// 索引条目的类型。老索引没有 kind，按单词书处理。
const bookKind = (b: RemoteBook) => b.kind ?? "words";

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

export function Wordbook({
  lang,
  onPractice,
  onChanged,
}: {
  lang: string;
  onPractice: () => void;
  onChanged?: () => void;
}) {
  const [rows, setRows] = useState<CardItem[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [importingArticle, setImportingArticle] = useState(false);
  const [articleDraft, setArticleDraft] = useState("");
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [skipped, setSkipped] = useState<string[]>([]);
  const [books, setBooks] = useState<BookInfo[]>([]);
  const [bookName, setBookName] = useState("");
  const [remote, setRemote] = useState<RemoteBook[] | null>(null);
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [indexUrl, setIndexUrl] = useState("");
  const [showIndex, setShowIndex] = useState(false);
  const [busyBook, setBusyBook] = useState<string | null>(null);
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
    api.listBooks().then(setBooks).catch(() => setBooks([]));
    api.booksIndexUrl().then(setIndexUrl).catch(() => {});
    api
      .remoteBooks()
      .then((r) => {
        setRemote(r);
        setRemoteError(null);
      })
      .catch((e) => setRemoteError(String(e)));
  }, [load]);

  const now = Math.floor(Date.now() / 1000);
  const customCount = (rows ?? []).filter((r) => r.is_custom).length;
  const dueCount = (rows ?? []).filter((r) => r.due <= now).length;
  const weakCount = (rows ?? []).filter((r) => r.mastery < 0.4).length;

  const visible = (rows ?? []).filter((r) => {
    const q = query.trim().toLowerCase();
    if (q && !r.surface.toLowerCase().includes(q) && !(r.gloss ?? "").toLowerCase().includes(q)) {
      return false;
    }
    if (filter === "due") return r.due <= now;
    if (filter === "weak") return r.mastery < 0.4;
    if (filter === "custom") return r.is_custom;
    return true;
  });

  const parsed = useMemo(() => parseRows(draft), [draft]);
  // 几千张卡一次渲染会卡，先画前 300 条
  const shown = visible.slice(0, 300);

  const doImport = async () => {
    setError(null);
    setSkipped([]);
    try {
      const r = await api.importWords(lang, draft, bookName.trim() || undefined);
      setNote(
        `新增 ${r.added} 个` + (r.updated ? `，更新 ${r.updated} 个` : "") +
          (r.skipped.length ? `，跳过 ${r.skipped.length} 行` : ""),
      );
      api.listBooks().then(setBooks).catch(() => {});
      setSkipped(r.skipped);
      setDraft("");
      setImporting(false);
      await load();
      onChanged?.();
    } catch (e) {
      setError(String(e));
    }
  };

  /// 阅读文章的导入：格式由内核解析，正文分词对齐不过就直接拒绝。
  const doImportArticle = async () => {
    setError(null);
    try {
      await api.importArticle(lang, articleDraft);
      setNote("文章已导入，「练习 · 阅读」里就能练");
      setArticleDraft("");
      setImportingArticle(false);
      onChanged?.();
    } catch (e) {
      setError(String(e));
    }
  };

  const importBook = async (id: string) => {
    setError(null);
    try {
      const r = await api.importBook(id);
      setNote(`单词书导入完成：新增 ${r.added} 个，更新 ${r.updated} 个`);
      await load();
      onChanged?.();
      api.listBooks().then(setBooks).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  };

  const installRemote = async (b: RemoteBook) => {
    setBusyBook(b.id);
    setError(null);
    try {
      const r = await api.installRemoteBook(b.lang, b.name, b.url, b.kind, b.mirrors);
      setNote(`${b.name}：新增 ${r.added} 个，更新 ${r.updated} 个`);
      setRemote((prev) =>
        prev?.map((x) => (x.id === b.id ? { ...x, imported: r.added + r.updated } : x)) ?? null,
      );
      await load();
      onChanged?.();
      api.listBooks().then(setBooks).catch(() => {});
    } catch (e) {
      setError(`${b.name} 导入失败：${e}`);
    } finally {
      setBusyBook(null);
    }
  };

  const refreshRemote = async () => {
    setRemoteError(null);
    setRemote(null);
    try {
      setRemote(await api.remoteBooks());
    } catch (e) {
      setRemoteError(String(e));
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
          <input
            className="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜词或释义"
          />
          <button onClick={() => setImporting((v) => !v)}>导入单词</button>
          <button
            onClick={() => {
              setImportingArticle((v) => !v);
              setImporting(false);
            }}
          >
            导入文章
          </button>
          <button className="primary" disabled={dueCount === 0} onClick={onPractice}>
            开始复习
          </button>
        </div>
      </div>

      {books.filter((b) => b.lang === lang).length > 0 && (
        <section className="books">
          <p className="section-label">单词书</p>
          <ul>
            {books
              .filter((b) => b.lang === lang)
              .map((b) => (
                <li key={b.id}>
                  <div>
                    <b>{b.name}</b>
                    <span className="muted">
                      {" "}
                      共 {b.count} 词
                      {b.imported > 0 ? ` · 已导入 ${b.imported}` : ""}
                    </span>
                  </div>
                  <button className="small" onClick={() => importBook(b.id)}>
                    {b.imported > 0 ? "重新导入" : "导入并开始背诵"}
                  </button>
                </li>
              ))}
          </ul>
        </section>
      )}

      <section className="books">
        <p className="section-label">在线单词书</p>
        <div className="books-toolbar">
          <button className="small" onClick={refreshRemote}>
            刷新索引
          </button>
          <button className="ghost" onClick={() => setShowIndex((v) => !v)}>
            索引设置
          </button>
        </div>
        {showIndex && (
          <div className="index-url">
            <input
              value={indexUrl}
              onChange={(e) => setIndexUrl(e.target.value)}
              placeholder="默认用仓库里的 books/index.json"
            />
            <button
              className="small"
              onClick={async () => {
                await api.setBooksIndexUrl(indexUrl).catch((e) => setError(String(e)));
                setShowIndex(false);
                refreshRemote();
              }}
            >
              保存并使用
            </button>
            <button
              className="ghost"
              onClick={async () => {
                setIndexUrl("");
                await api.setBooksIndexUrl("").catch((e) => setError(String(e)));
                setShowIndex(false);
                refreshRemote();
              }}
            >
              恢复默认
            </button>
          </div>
        )}
        {remoteError && <p className="muted">在线索引不可用：{remoteError}（不影响本地导入）</p>}
        {remote === null && !remoteError && <p className="muted">读取索引…</p>}
        {remote?.length === 0 && <p className="muted">索引里没有单词书</p>}
        {!!remote?.length &&
          remote.filter((b) => b.lang === lang && bookKind(b) === "words").length === 0 && (
            <p className="muted">索引里暂时没有这门语言的单词书</p>
          )}
        {!!remote?.length && (
          <ul>
            {remote
              .filter((b) => b.lang === lang && bookKind(b) === "words")
              .map((b) => (
                <li key={b.id}>
                  <div>
                    <b>{b.name}</b>
                    {b.imported > 0 && <span className="tag">已下载 {b.imported}</span>}
                    <span className="muted">
                      {" "}
                      {b.count ? `${b.count} 词 · ` : ""}
                      {b.license || "许可见来源"}
                    </span>
                    {b.note && <div className="muted">{b.note}</div>}
                  </div>
                  <button
                    className="small"
                    disabled={busyBook === b.id}
                    onClick={() => installRemote(b)}
                  >
                    {busyBook === b.id ? "下载中…" : b.imported > 0 ? "重新导入" : "下载并导入"}
                  </button>
                </li>
              ))}
          </ul>
        )}
      </section>

      {!!remote?.length &&
        remote.filter((b) => b.lang === lang && bookKind(b) === "sentences").length > 0 && (
          <section className="books">
            <p className="section-label">在线句库</p>
            <p className="muted">句库下载后进「拼句子」，按句子卡入队，随时可在数据里清掉。</p>
            <ul>
              {remote
                .filter((b) => b.lang === lang && bookKind(b) === "sentences")
                .map((b) => (
                  <li key={b.id}>
                    <div>
                      <b>{b.name}</b>
                      {b.imported > 0 && <span className="tag">已下载 {b.imported}</span>}
                      <span className="muted">
                        {" "}
                        {b.count ? `${b.count} 句 · ` : ""}
                        {b.license || "许可见来源"}
                      </span>
                      {b.note && <div className="muted">{b.note}</div>}
                    </div>
                    <button
                      className="small"
                      disabled={busyBook === b.id}
                      onClick={() => installRemote(b)}
                    >
                      {busyBook === b.id ? "下载中…" : b.imported > 0 ? "重新导入" : "下载并导入"}
                    </button>
                  </li>
                ))}
            </ul>
          </section>
        )}

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
            <input
              className="book-name"
              value={bookName}
              onChange={(e) => setBookName(e.target.value)}
              placeholder="书名（可省略）"
            />
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

      {importingArticle && (
        <div className="import-box">
          <p className="muted">
            粘贴一篇文章：<code># 标题</code> 开头，正文直接写、空行分段；题目用
            <code>? 题干</code>、<code>- 选项</code>、<code>= 正确选项序号</code>（从 1 开始）。
          </p>
          <textarea
            value={articleDraft}
            onChange={(e) => setArticleDraft(e.target.value)}
            placeholder={
              "# 为什么我们玩消失\n\nGhosting used to be rude. Now it is normal.\n\n? 人们不回消息的原因是什么？\n- 对话太沉重\n- 手机没电\n- 大家都变礼貌\n= 1"
            }
          />
          <div className="import-file">
            <label className="file-btn">
              从文件读取
              <input
                type="file"
                accept=".txt,.md"
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (f) setArticleDraft(await f.text());
                  e.target.value = "";
                }}
              />
            </label>
            <div className="spacer" />
            <button className="primary" disabled={!articleDraft.trim()} onClick={doImportArticle}>
              导入文章
            </button>
          </div>
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
        {shown.map((r) => (
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
      {visible.length > shown.length && (
        <p className="muted">
          只显示前 {shown.length} 条，还有 {visible.length - shown.length} 条——用上面的筛选缩小范围。
        </p>
      )}
    </div>
  );
}

function dueLabel(due: number, now: number) {
  if (due <= now) return "待复习";
  const days = Math.round((due - now) / 86_400);
  if (days <= 1) return "明天";
  return `${days} 天后`;
}