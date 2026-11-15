import { useEffect, useMemo, useRef, useState } from "react";
import { api, type DayActivity } from "./api";

const WORK_MINUTES = [15, 25, 45] as const;
const BREAK_MINUTES = 5;
const DAYS = 140; // 20 周

export function Pomodoro() {
  const [minutes, setMinutes] = useState<number>(25);
  const [phase, setPhase] = useState<"work" | "break">("work");
  const [left, setLeft] = useState(25 * 60);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(0);
  const tick = useRef<number | null>(null);

  const total = (phase === "work" ? minutes : BREAK_MINUTES) * 60;

  useEffect(() => {
    if (!running) return;
    tick.current = window.setInterval(() => setLeft((v) => Math.max(0, v - 1)), 1000);
    return () => {
      if (tick.current) window.clearInterval(tick.current);
    };
  }, [running]);

  // 归零：工作段自然结束才记一次专注
  useEffect(() => {
    if (left !== 0) return;
    setRunning(false);
    if (phase === "work") {
      api.recordFocus(minutes, true).catch(() => {});
      setDone((d) => d + 1);
      setPhase("break");
      setLeft(BREAK_MINUTES * 60);
    } else {
      setPhase("work");
      setLeft(minutes * 60);
    }
  }, [left, phase, minutes]);

  const mm = String(Math.floor(left / 60)).padStart(2, "0");
  const ss = String(left % 60).padStart(2, "0");
  const ratio = total === 0 ? 0 : 1 - left / total;
  const R = 54;
  const C = 2 * Math.PI * R;

  return (
    <div className="pomodoro compact">
      <div className="phase-switch">
        <button
          className={phase === "work" ? "on" : ""}
          onClick={() => {
            setRunning(false);
            setPhase("work");
            setLeft(minutes * 60);
          }}
        >
          专注
        </button>
        <button
          className={phase === "break" ? "on" : ""}
          onClick={() => {
            setRunning(false);
            setPhase("break");
            setLeft(BREAK_MINUTES * 60);
          }}
        >
          休息
        </button>
      </div>

      <div className="ring small">
        <svg width="132" height="132" viewBox="0 0 132 132">
          <circle cx="66" cy="66" r={R} className="ring-track" />
          <circle
            cx="66"
            cy="66"
            r={R}
            className={`ring-fill ${phase}`}
            strokeDasharray={C}
            strokeDashoffset={C * (1 - ratio)}
            transform="rotate(-90 66 66)"
          />
        </svg>
        <div className="ring-label">
          <span className="time">
            {mm}:{ss}
          </span>
        </div>
      </div>

      <div className="durations">
        {WORK_MINUTES.map((m) => (
          <button
            key={m}
            className={minutes === m ? "on" : ""}
            disabled={running}
            onClick={() => {
              setMinutes(m);
              if (phase === "work" && !running) setLeft(m * 60);
            }}
          >
            {m}
          </button>
        ))}
      </div>

      <div className="actions center">
        <button className="primary small" onClick={() => setRunning((v) => !v)}>
          {running ? "暂停" : left === total ? "开始" : "继续"}
        </button>
      </div>
      <p className="muted">今天 {done} 个番茄</p>
    </div>
  );
}

export function Checkin() {
  const [activity, setActivity] = useState<DayActivity[]>([]);

  useEffect(() => {
    api
      .activity(DAYS, -new Date().getTimezoneOffset())
      .then(setActivity)
      .catch(() => {});
  }, []);

  const stats = useMemo(() => summarize(activity), [activity]);

  return (
    <div className="checkin compact">
      <div className="checkin-head">
        <p className="section-label">打卡</p>
        <span className="muted">
          连续 {stats.streak} 天 · 今日 {stats.todayAttempts} 题
        </span>
      </div>

      <div className="heatmap">
        {padStart(activity).map((d, i) =>
          d === null ? (
            <div key={`pad-${i}`} className="cell empty" />
          ) : (
            <div
              key={d.date}
              className={`cell l${level(d)}`}
              title={`${d.date} · 答题 ${d.attempts} · 复习 ${d.reviews} · 专注 ${d.focus_minutes} 分钟`}
            />
          ),
        )}
      </div>

      <div className="legend">
        <span className="muted">少</span>
        {[0, 1, 2, 3, 4].map((l) => (
          <div key={l} className={`cell l${l}`} />
        ))}
        <span className="muted">多</span>
      </div>
    </div>
  );
}

function level(d: DayActivity) {
  const score = d.attempts + d.reviews + d.focus_minutes / 5;
  if (score <= 0) return 0;
  if (score < 3) return 1;
  if (score < 8) return 2;
  if (score < 16) return 3;
  return 4;
}

/// 让第一列对齐到周一：前面补空格子
function padStart(days: DayActivity[]): (DayActivity | null)[] {
  if (days.length === 0) return [];
  const first = new Date(days[0].date + "T00:00:00");
  const weekday = (first.getDay() + 6) % 7; // 周一=0
  return [...Array<null>(weekday).fill(null), ...days];
}

function summarize(days: DayActivity[]) {
  let streak = 0;
  for (let i = days.length - 1; i >= 0; i--) {
    const d = days[i];
    if (d.attempts + d.reviews + d.focus_minutes > 0) streak++;
    else break;
  }
  const today = days[days.length - 1];
  return {
    streak,
    todayAttempts: today?.attempts ?? 0,
  };
}