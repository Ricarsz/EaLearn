import { useCallback, useEffect, useState } from "react";
import { api, type PackSummary, type Stats } from "./api";
import { Home, type Mode } from "./Home";
import { Practice } from "./Practice";
import { Scenario } from "./Scenario";
import { Settings } from "./Settings";
import { Wordbook } from "./Wordbook";
import { Phonetics } from "./Phonetics";
import { Stats as StatsPage } from "./Stats";

type Tab = "practice" | "scenario" | "wordbook" | "phonetics" | "stats";

const TABS: { id: Tab; label: string; hint: string }[] = [
  { id: "practice", label: "练习", hint: "组卷 · 复习 · 打卡" },
  { id: "scenario", label: "情景", hint: "剧本对话与自由聊" },
  { id: "wordbook", label: "词库", hint: "卡片、单词书与导入" },
  { id: "phonetics", label: "发音", hint: "音标 · 字母表" },
  { id: "stats", label: "统计", hint: "趋势 · 错题 · 负荷" },
];

const MODE_TITLE: Record<Mode, string> = {
  mixed: "混合复习",
  recall: "背单词",
  audio: "听力",
  sentence: "拼句子",
  article: "阅读",
  weak: "错题重练",
};

export function App() {
  const [packs, setPacks] = useState<PackSummary[]>([]);
  const [lang, setLang] = useState<string | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);
  const [tab, setTab] = useState<Tab>("practice");
  const [mode, setMode] = useState<Mode | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
  const tabMeta = TABS.find((t) => t.id === tab) ?? TABS[0];

  const go = (next: Tab) => {
    setTab(next);
    setMode(null);
  };

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="dot" />
          <h1>EaLearn</h1>
        </div>

        <section>
          <p className="section-label">语言</p>
          <div className="packs">
            {packs.length === 0 && <p className="muted">没找到语言包</p>}
            {packs.map((p) => (
              <button
                key={p.lang}
                className={`pack${p.lang === lang ? " active" : ""}`}
                onClick={() => {
                  setLang(p.lang);
                  go("practice");
                  refresh();
                }}
              >
                <b>{p.display}</b>
                <span className="muted">
                  {p.words} 词 · {p.sentences} 句 · {p.articles} 篇 · {p.dialogs} 段
                </span>
              </button>
            ))}
          </div>
        </section>

        <nav className="nav">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`nav-item${tab === t.id ? " active" : ""}`}
              onClick={() => go(t.id)}
            >
              <span className="nav-label">{t.label}</span>
              <span className="nav-hint">{t.hint}</span>
            </button>
          ))}
        </nav>

        {stats && (
          <section className="side-stats">
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

        <button className="settings-btn" onClick={() => setSettingsOpen(true)}>
          ⚙ 设置
        </button>
      </aside>

      <main className="main">
        <header className="topbar">
          <div className="topbar-title">
            <h2>{mode ? MODE_TITLE[mode] : tabMeta.label}</h2>
            <span className="muted">
              {mode === "mixed"
                ? "到期卡先排 · 其余随机补足 · 答错自动入卡"
                : mode
                  ? "只出这种题 · 答错自动入卡"
                  : tabMeta.hint}
            </span>
          </div>
          <div className="topbar-meta">
            {pack && <span className="pill">{pack.display}</span>}
            {stats && <span className="pill">待复习 {stats.due_now}</span>}
            {stats && <span className="pill">今日 {stats.attempts_today} 题</span>}
          </div>
        </header>

        <div className={`content ${tab}${mode ? " session" : ""}`}>
          {error && <p className="error panel">{error}</p>}
          {!lang && <p className="panel muted">没有可用的语言包。</p>}

          {lang && tab === "practice" && mode === null && (
            <Home
              key={`${lang}-home`}
              lang={lang}
              pack={pack}
              stats={stats}
              onStart={(m) => setMode(m)}
            />
          )}
          {lang && tab === "practice" && mode !== null && (
            <Practice
              key={`${lang}-${mode}-practice`}
              lang={lang}
              mode={mode}
              onExit={() => {
                setMode(null);
                refresh();
              }}
              onFinished={refresh}
            />
          )}
          {lang && tab === "scenario" && (
            <Scenario
              key={`${lang}-scenario`}
              lang={lang}
              onOpenSettings={() => setSettingsOpen(true)}
            />
          )}
          {lang && tab === "wordbook" && (
            <Wordbook
              key={`${lang}-wordbook`}
              lang={lang}
              onPractice={() => {
                setTab("practice");
                setMode("mixed");
              }}
              onChanged={refresh}
            />
          )}
          {lang && tab === "phonetics" && <Phonetics key={`${lang}-phonetics`} lang={lang} />}
          {lang && tab === "stats" && (
            <StatsPage
              key={`${lang}-stats`}
              lang={lang}
              onPracticeWeak={() => {
                setTab("practice");
                setMode("weak");
              }}
            />
          )}
        </div>
      </main>

      {settingsOpen && (
        <Settings
          lang={lang}
          onClose={() => setSettingsOpen(false)}
          onDataReset={() => {
            refresh();
            setMode(null);
          }}
        />
      )}
    </div>
  );
}