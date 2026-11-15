import { useEffect, useMemo, useState } from "react";
import { api, type DayAccuracy, type DueBucket, type KindAccuracy, type WeakEntry } from "./api";

const KIND_LABEL: Record<string, string> = {
  word: "看词选义",
  "word-from-audio": "听音选词",
  recall: "自评回忆",
  sentence: "拼句子",
  "article-mcq": "阅读理解",
};

function shortDate(ymd: string): string {
  const [, m, d] = ymd.split("-");
  return `${Number(m)}/${Number(d)}`;
}

export function Stats({ lang, onPracticeWeak }: { lang: string; onPracticeWeak: () => void }) {
  const tz = useMemo(() => -new Date().getTimezoneOffset(), []);
  const [days, setDays] = useState(14);
  const [daily, setDaily] = useState<DayAccuracy[]>([]);
  const [kinds, setKinds] = useState<KindAccuracy[]>([]);
  const [forecast, setForecast] = useState<DueBucket[]>([]);
  const [weak, setWeak] = useState<WeakEntry[]>([]);

  useEffect(() => {
    let alive = true;
    api
      .dailyAccuracy(days, tz)
      .then((d) => alive && setDaily(d))
      .catch(() => {});
    api
      .kindAccuracy(days, lang)
      .then((k) => alive && setKinds(k))
      .catch(() => {});
    api
      .dueForecast(7, tz, lang)
      .then((f) => alive && setForecast(f))
      .catch(() => {});
    api
      .weakEntries(lang, 30, 40)
      .then((w) => alive && setWeak(w))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [lang, days, tz]);

  const peak = Math.max(1, ...daily.map((d) => d.attempts));
  const totalAttempts = daily.reduce((a, d) => a + d.attempts, 0);
  const totalCorrect = daily.reduce((a, d) => a + d.correct, 0);
  const accuracy = totalAttempts > 0 ? Math.round((totalCorrect / totalAttempts) * 100) : 0;
  const duePeak = Math.max(1, ...forecast.map((f) => f.due));
  const dueWeek = forecast.reduce((a, f) => a + f.due, 0);

  return (
    <div className="panel wide stats-page">
      <div className="mode-switch">
        {[7, 14, 30].map((d) => (
          <button key={d} className={days === d ? "active" : ""} onClick={() => setDays(d)}>
            近 {d} 天
          </button>
        ))}
        <span className="muted stats-summary">
          {totalAttempts} 题 · 正确率 {accuracy}%{totalAttempts === 0 ? "（还没有作答）" : ""}
        </span>
      </div>

      <section>
        <p className="section-label">每日作答</p>
        <div className="bars">
          {daily.map((d) => {
            const okH = Math.round((d.correct / peak) * 100);
            const badH = Math.round(((d.attempts - d.correct) / peak) * 100);
            const ok = d.attempts > 0 ? Math.round((d.correct / d.attempts) * 100) : 0;
            return (
              <div className="bar-col" key={d.date} title={`${d.date} · ${d.attempts} 题 · 正确率 ${ok}%`}>
                <div className="bar-track">
                  <div className="bar-ok" style={{ height: `${okH}%` }} />
                  <div className="bar-wrong" style={{ height: `${badH}%` }} />
                </div>
                <span className="bar-label">{shortDate(d.date)}</span>
              </div>
            );
          })}
        </div>
        <p className="muted legend">
          <span className="swatch ok" /> 答对 <span className="swatch bad" /> 答错
        </p>
      </section>

      <div className="stats-grid">
        <section>
          <p className="section-label">题型表现（近 {days} 天）</p>
          {kinds.length === 0 && <p className="muted">这段时间没有作答。</p>}
          {kinds.length > 0 && (
            <table className="jamo-table">
              <thead>
                <tr>
                  <th>题型</th>
                  <th>题量</th>
                  <th>正确率</th>
                  <th>均耗时</th>
                </tr>
              </thead>
              <tbody>
                {kinds.map((k) => (
                  <tr key={k.kind}>
                    <td>{KIND_LABEL[k.kind] ?? k.kind}</td>
                    <td>{k.attempts}</td>
                    <td>{Math.round((k.correct / Math.max(1, k.attempts)) * 100)}%</td>
                    <td className="muted">{(k.avg_ms / 1000).toFixed(1)}s</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>

        <section>
          <p className="section-label">未来 7 天到期</p>
          <div className="bars small">
            {forecast.map((f) => (
              <div className="bar-col" key={f.date} title={`${f.date} · ${f.due} 张`}>
                <div className="bar-track">
                  <div className="bar-due" style={{ height: `${Math.round((f.due / duePeak) * 100)}%` }} />
                </div>
                <span className="bar-label">{shortDate(f.date)}</span>
              </div>
            ))}
          </div>
          <p className="muted">共 {dueWeek} 张 · 今天到期 {forecast[0]?.due ?? 0} 张</p>
        </section>
      </div>

      <section>
        <p className="section-label">错题本（近 30 天）</p>
        {weak.length === 0 && <p className="muted">没有错题，保持住。</p>}
        {weak.length > 0 && (
          <>
            <div className="weak-head">
              <span className="muted">按错次排序，最多 40 条</span>
              <button className="small" onClick={onPracticeWeak}>
                错题重练
              </button>
            </div>
            <ul className="weak-list">
              {weak.map((w) => (
                <li key={w.entry_id}>
                  <div>
                    <b>{w.surface}</b>
                    {w.gloss && <span className="muted"> {w.gloss}</span>}
                  </div>
                  <span className="tag">
                    错 {w.wrong} / {w.total}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>
    </div>
  );
}