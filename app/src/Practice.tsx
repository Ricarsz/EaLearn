import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Answer, type Exercise, type SubmitResult } from "./api";
import { playUrl } from "./audio";

const SESSION_SIZE = 8;

export function Practice({ lang }: { lang: string }) {
  const [session, setSession] = useState<Exercise[] | null>(null);
  const [idx, setIdx] = useState(0);
  const [placed, setPlaced] = useState<number[]>([]);
  const [picked, setPicked] = useState<number | null>(null);
  const [result, setResult] = useState<SubmitResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const started = useRef(Date.now());

  const ex = session?.[idx];

  const reset = useCallback(() => {
    setPlaced([]);
    setPicked(null);
    setResult(null);
    started.current = Date.now();
  }, []);

  const load = useCallback(async () => {
    setError(null);
    setSession(null);
    try {
      const s = await api.startSession(lang, SESSION_SIZE, Date.now() >>> 0);
      setSession(s);
      setIdx(0);
      reset();
    } catch (e) {
      setError(String(e));
    }
  }, [lang, reset]);

  useEffect(() => {
    load();
  }, [load]);

  const submit = async (answer: Answer) => {
    if (!ex) return;
    try {
      setResult(await api.submit(lang, ex, answer, Date.now() - started.current));
    } catch (e) {
      setError(String(e));
    }
  };

  const advance = () => {
    if (!session) return;
    if (idx + 1 >= session.length) {
      setIdx(session.length);
      return;
    }
    setIdx(idx + 1);
    reset();
  };

  if (error) {
    return (
      <div className="panel">
        <p className="error">出错了：{error}</p>
        <button onClick={load}>重试</button>
      </div>
    );
  }

  if (!session) return <div className="panel muted">正在组卷…</div>;

  if (session.length === 0) {
    return (
      <div className="panel muted">
        这个语言包还没有能出题的素材。先补齐句子、图片或音频再练。
      </div>
    );
  }

  if (idx >= session.length) {
    return (
      <div className="panel done">
        <div className="done-mark">✓</div>
        <h2>本组完成</h2>
        <p className="muted">共 {session.length} 题</p>
        <button className="primary" onClick={load}>
          再来一组
        </button>
      </div>
    );
  }

  if (!ex) return <div className="panel muted">本组已结束。</div>;

  const ready =
    ex.kind === "build-sentence" ? placed.length === ex.chunks.length : picked !== null;

  const progress = ((idx + (result ? 1 : 0)) / session.length) * 100;

  return (
    <div className="panel">
      <div className="progress">
        <div className="bar">
          <div className="fill" style={{ width: `${progress}%` }} />
        </div>
        <span className="muted">
          {idx + 1} / {session.length} · {kindLabel(ex)}
        </span>
      </div>

      <ExerciseView
        lang={lang}
        ex={ex}
        placed={placed}
        setPlaced={setPlaced}
        picked={picked}
        setPicked={setPicked}
        locked={!!result}
      />

      {result ? (
        <Feedback result={result} onNext={advance} lang={lang} />
      ) : (
        <div className="actions">
          <button
            className="primary"
            disabled={!ready}
            onClick={() =>
              ex.kind === "build-sentence"
                ? submit({ kind: "order", order: placed })
                : submit({ kind: "choice", index: picked ?? 0 })
            }
          >
            提交
          </button>
        </div>
      )}
    </div>
  );
}

function kindLabel(ex: Exercise) {
  switch (ex.kind) {
    case "build-sentence":
      return "拼句";
    case "word-from-image":
      return "看图选词";
    case "word-from-audio":
      return "听音选词";
    case "article-mcq":
      return "阅读理解";
    default:
      return "情景";
  }
}

function ExerciseView({
  lang,
  ex,
  placed,
  setPlaced,
  picked,
  setPicked,
  locked,
}: {
  lang: string;
  ex: Exercise;
  placed: number[];
  setPlaced: (v: number[]) => void;
  picked: number | null;
  setPicked: (v: number | null) => void;
  locked: boolean;
}) {
  if (ex.kind === "build-sentence") {
    return (
      <SentenceBuilder
        ex={ex}
        placed={placed}
        setPlaced={setPlaced}
        locked={locked}
        lang={lang}
      />
    );
  }

  if (ex.kind === "word-from-image") {
    return (
      <PictureChoice
        lang={lang}
        image={ex.image}
        options={ex.options}
        picked={picked}
        setPicked={setPicked}
        locked={locked}
      />
    );
  }

  if (ex.kind === "word-from-audio") {
    const answer = ex.options[ex.answer];
    return (
      <div className="exercise">
        <PlayButton lang={lang} text={answer} entryId={ex.entry} big />
        <Options options={ex.options} picked={picked} setPicked={setPicked} locked={locked} />
      </div>
    );
  }

  if (ex.kind === "chat-scenario") {
    return <p className="muted">情景题在「情景」页练习。</p>;
  }

  return (
    <div className="exercise">
      <p className="question">{ex.prompt}</p>
      <Options options={ex.options} picked={picked} setPicked={setPicked} locked={locked} />
    </div>
  );
}

/// 拼句：词块既能点，也能拖。
///
/// 拖拽用 HTML5 DnD（WebKitGTK 支持），载荷是 `pool:<下标>` 或 `ans:<位置>`：
/// 从词库拖到答案区是插入，答案区内部拖动是重排，拖回词库是移除。
function SentenceBuilder({
  lang,
  ex,
  placed,
  setPlaced,
  locked,
}: {
  lang: string;
  ex: Extract<Exercise, { kind: "build-sentence" }>;
  placed: number[];
  setPlaced: (v: number[]) => void;
  locked: boolean;
}) {
  const [over, setOver] = useState<number | null>(null);

  const dropOnAnswer = (pos: number | null) => (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOver(null);
    if (locked) return;
    const [kind, raw] = e.dataTransfer.getData("text/plain").split(":");
    const from = Number(raw);
    if (Number.isNaN(from)) return;

    if (kind === "pool") {
      if (placed.includes(from)) return;
      const next = [...placed];
      next.splice(pos === null ? next.length : Math.min(pos, next.length), 0, from);
      setPlaced(next);
    } else if (kind === "ans" && pos !== null && pos !== from) {
      const next = [...placed];
      const [moved] = next.splice(from, 1);
      next.splice(pos > from ? pos - 1 : pos, 0, moved);
      setPlaced(next);
    }
  };

  const dropOnPool = (e: React.DragEvent) => {
    e.preventDefault();
    if (locked) return;
    const [kind, raw] = e.dataTransfer.getData("text/plain").split(":");
    if (kind === "ans") {
      const at = Number(raw);
      setPlaced(placed.filter((_, i) => i !== at));
    }
  };

  return (
    <div className="exercise">
      {ex.prompt && <p className="gloss">{ex.prompt}</p>}

      <div
        className={`answer-line${over === -1 ? " drag-over" : ""}`}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(-1);
        }}
        onDragLeave={() => setOver(null)}
        onDrop={dropOnAnswer(null)}
      >
        {placed.length === 0 && <span className="muted">把词块拖到这里，或直接点击</span>}
        {placed.map((ci, pos) => (
          <button
            key={`${ci}-${pos}`}
            className={`chip placed${over === pos ? " drag-over" : ""}`}
            draggable={!locked}
            onDragStart={(e) => {
              e.dataTransfer.setData("text/plain", `ans:${pos}`);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setOver(pos);
            }}
            onDrop={dropOnAnswer(pos)}
            disabled={locked}
            onClick={() => setPlaced(placed.filter((_, p) => p !== pos))}
          >
            {ex.chunks[ci]}
          </button>
        ))}
      </div>

      <div className="pool" onDragOver={(e) => e.preventDefault()} onDrop={dropOnPool}>
        {ex.chunks.map((c, i) => (
          <button
            key={i}
            className="chip"
            draggable={!locked && !placed.includes(i)}
            onDragStart={(e) => {
              e.dataTransfer.setData("text/plain", `pool:${i}`);
              e.dataTransfer.effectAllowed = "move";
            }}
            disabled={locked || placed.includes(i)}
            onClick={() => setPlaced([...placed, i])}
          >
            {c}
          </button>
        ))}
      </div>

      <div className="hint muted">
        <PlayButton
          lang={lang}
          text={ex.answer.map((i) => ex.chunks[i]).join(" ")}
        />
        <span>拖动词块调整顺序，点答案里的词块可以拿回来</span>
      </div>
    </div>
  );
}

function PictureChoice({
  lang,
  image,
  options,
  picked,
  setPicked,
  locked,
}: {
  lang: string;
  image: string;
  options: string[];
  picked: number | null;
  setPicked: (v: number | null) => void;
  locked: boolean;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .assetData(lang, image)
      .then((d) => alive && setUrl(d))
      .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [lang, image]);

  return (
    <div className="exercise">
      <div className="picture">
        {url ? <img src={url} alt="" /> : <span className="muted">（图片素材待生成）</span>}
      </div>
      <Options options={options} picked={picked} setPicked={setPicked} locked={locked} />
    </div>
  );
}

function PlayButton({
  lang,
  text,
  entryId,
  big,
}: {
  lang: string;
  text: string;
  entryId?: string;
  big?: boolean;
}) {
  const [state, setState] = useState<"idle" | "loading" | "error">("idle");

  const play = async () => {
    if (state === "loading") return;
    setState("loading");
    try {
      const url = await api.speak(lang, text, entryId);
      await playUrl(url);
      setState("idle");
    } catch {
      setState("error");
    }
  };

  return (
    <button
      className={`play${big ? " big" : ""}${state === "error" ? " failed" : ""}`}
      onClick={play}
      title={state === "error" ? "合成失败，检查音色设置" : "播放"}
    >
      {state === "loading" ? "…" : state === "error" ? "⚠" : "▶"}
    </button>
  );
}

function Options({
  options,
  picked,
  setPicked,
  locked,
}: {
  options: string[];
  picked: number | null;
  setPicked: (v: number | null) => void;
  locked: boolean;
}) {
  return (
    <div className="options">
      {options.map((o, i) => (
        <button
          key={i}
          className={`option${picked === i ? " picked" : ""}`}
          disabled={locked}
          onClick={() => setPicked(i)}
        >
          <span className="key">{String.fromCharCode(65 + i)}</span>
          {o}
        </button>
      ))}
    </div>
  );
}

function Feedback({
  result,
  onNext,
  lang,
}: {
  result: SubmitResult;
  onNext: () => void;
  lang: string;
}) {
  const { grade, card } = result;
  const days = card ? Math.max(0, Math.round((card.state.due * 1000 - Date.now()) / 86_400_000)) : null;
  const expected = grade.items.find((i) => !i.correct && i.expected)?.expected;

  return (
    <div className={`feedback ${grade.correct ? "ok" : "bad"}`}>
      <div className="feedback-head">
        <p className="verdict">
          {grade.correct ? "正确" : grade.ratio > 0 ? `部分正确 ${Math.round(grade.ratio * 100)}%` : "再想想"}
        </p>
        {expected && (
          <span className="answer-reveal">
            应为 <b>{expected}</b>
            <PlayButton lang={lang} text={expected} />
          </span>
        )}
      </div>

      {!grade.correct && (
        <ul className="correction">
          {grade.items
            .filter((i) => !i.correct && i.expected)
            .slice(0, 4)
            .map((i) => (
              <li key={i.index}>
                第 {i.index + 1} 处应为 <b>{i.expected}</b>
              </li>
            ))}
        </ul>
      )}

      {card && days !== null && (
        <p className="muted">
          已入复习队列：{card.label} · {days} 天后再见
        </p>
      )}

      <div className="actions">
        <button className="primary" onClick={onNext}>
          下一题
        </button>
      </div>
    </div>
  );
}