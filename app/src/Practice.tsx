import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Answer, type Exercise, type SubmitResult } from "./api";

const SESSION_SIZE = 8;

function useAsset(lang: string, rel: string | undefined) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (!rel) return;
    api
      .assetData(lang, rel)
      .then((d) => alive && setUrl(d))
      .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [lang, rel]);
  return url;
}

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
      const r = await api.submit(lang, ex, answer, Date.now() - started.current);
      setResult(r);
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
        <h2>本组完成</h2>
        <p className="muted">共 {session.length} 题</p>
        <button className="primary" onClick={load}>
          再来一组
        </button>
      </div>
    );
  }

  if (!ex) {
    return <div className="panel muted">本组已结束。</div>;
  }

  const ready =
    ex.kind === "build-sentence"
      ? placed.length === ex.chunks.length
      : picked !== null;

  return (
    <div className="panel">
      <div className="progress">
        <span>
          {idx + 1} / {session.length}
        </span>
        <span className="kind">{kindLabel(ex)}</span>
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
        <Feedback result={result} onNext={advance} />
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
      <div className="exercise">
        {ex.prompt && <p className="gloss">{ex.prompt}</p>}
        <div className="answer-line">
          {placed.length === 0 && <span className="muted">点下面的词块拼成句子</span>}
          {placed.map((ci, pos) => (
            <button
              key={`${ci}-${pos}`}
              className="chip placed"
              disabled={locked}
              onClick={() => setPlaced(placed.filter((_, p) => p !== pos))}
            >
              {ex.chunks[ci]}
            </button>
          ))}
        </div>
        <div className="pool">
          {ex.chunks.map((c, i) => (
            <button
              key={i}
              className="chip"
              disabled={locked || placed.includes(i)}
              onClick={() => setPlaced([...placed, i])}
            >
              {c}
            </button>
          ))}
        </div>
      </div>
    );
  }

  if (ex.kind === "word-from-image") {
    return <PictureChoice lang={lang} image={ex.image} options={ex.options} picked={picked} setPicked={setPicked} locked={locked} />;
  }

  if (ex.kind === "word-from-audio") {
    return (
      <div className="exercise">
        <AudioButton lang={lang} file={ex.audio} />
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
  const url = useAsset(lang, image);
  return (
    <div className="exercise">
      <div className="picture">
        {url ? <img src={url} alt="" /> : <span className="muted">（图片素材待生成）</span>}
      </div>
      <Options options={options} picked={picked} setPicked={setPicked} locked={locked} />
    </div>
  );
}

function AudioButton({ lang, file }: { lang: string; file: string }) {
  const url = useAsset(lang, file);
  return (
    <button
      className="audio"
      disabled={!url}
      onClick={() => url && new Audio(url).play()}
    >
      {url ? "▶ 播放" : "（音频素材待生成）"}
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
          {o}
        </button>
      ))}
    </div>
  );
}

function Feedback({ result, onNext }: { result: SubmitResult; onNext: () => void }) {
  const { grade, card } = result;
  const days = card
    ? Math.max(0, Math.round((card.state.due * 1000 - Date.now()) / 86_400_000))
    : null;

  return (
    <div className={`feedback ${grade.correct ? "ok" : "bad"}`}>
      <p className="verdict">
        {grade.correct ? "正确" : grade.ratio > 0 ? `部分正确 ${Math.round(grade.ratio * 100)}%` : "再想想"}
      </p>
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