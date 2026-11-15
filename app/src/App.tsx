import { useCallback, useEffect, useState } from "react";
import { api, type PackSummary, type Stats } from "./api";
import { Practice } from "./Practice";
import { Scenario } from "./Scenario";

type Tab = "practice" | "scenario";

export function App() {
  const [packs, setPacks] = useState<PackSummary[]>([]);
  const [lang, setLang] = useState<string | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);
  const [tab, setTab] = useState<Tab>("practice");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listPacks();
      setPacks(list);
      setLang((cur) => cur ?? list[0]?.lang ?? null);
      setStats(await api.stats());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const pack = packs.find((p) => p.lang === lang);

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>EaLearn</h1>

        <section className="packs">
          {packs.length === 0 && <p className="muted">没找到语言包</p>}
          {packs.map((p) => (
            <button
              key={p.lang}
              className={`pack${p.lang === lang ? " active" : ""}`}
              onClick={() => {
                setLang(p.lang);
                setTab("practice");
                refresh();
              }}
            >
              <b>{p.display}</b>
              <span className="muted">
                {p.words} 词 · {p.sentences} 句 · {p.articles} 篇 · {p.dialogs} 段
              </span>
            </button>
          ))}
        </section>

        {stats && (
          <section className="stats">
            <div>
              <span className="num">{stats.due_now}</span>
              <span className="muted">待复习</span>
            </div>
            <div>
              <span className="num">{stats.cards}</span>
              <span className="muted">总卡数</span>
            </div>
            <div>
              <span className="num">{Math.round(stats.accuracy_today * 100)}%</span>
              <span className="muted">今日正确率</span>
            </div>
          </section>
        )}

        {pack && (
          <section className="coverage">
            <p className="muted">可用题型</p>
            <ul>
              <li>拼句 {pack.coverage.buildSentence}</li>
              <li>看图 {pack.coverage.wordFromImage}</li>
              <li>听音 {pack.coverage.wordFromAudio}</li>
              <li>阅读 {pack.coverage.articleMcq}</li>
            </ul>
          </section>
        )}
      </aside>

      <main className="main">
        <nav className="tabs">
          <button className={tab === "practice" ? "active" : ""} onClick={() => setTab("practice")}>
            练习
          </button>
          <button className={tab === "scenario" ? "active" : ""} onClick={() => setTab("scenario")}>
            情景
          </button>
        </nav>

        {error && <p className="error">{error}</p>}
        {!lang && <p className="panel muted">没有可用的语言包。</p>}
        {lang && tab === "practice" && <Practice key={`${lang}-practice`} lang={lang} />}
        {lang && tab === "scenario" && <Scenario key={`${lang}-scenario`} lang={lang} />}
      </main>
    </div>
  );
}