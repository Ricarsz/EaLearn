import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Answer, type Article, type Exercise, type SessionPlan, type SubmitResult } from "./api";
import { playUrl } from "./audio";
import { loadGoal } from "./Goal";

const SESSION_SIZE = 8;

export function Practice({
  lang,
  mode,
  onExit,
  onFinished,
}: {
  lang: string;
  mode: string;
  onExit: () => void;
  onFinished?: () => void;
}) {
  const [session, setSession] = useState<SessionPlan | null>(null);
  const [idx, setIdx] = useState(0);
  const [placed, setPlaced] = useState<number[]>([]);
  const [picked, setPicked] = useState<number | null>(null);
  const [result, setResult] = useState<SubmitResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const started = useRef(Date.now());

  const ex = session?.exercises?.[idx];

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
      // 有每日题量目标时，这一组按「今天还差多少题」出，练完就达标。
      let size = SESSION_SIZE;
      try {
        const goal = await loadGoal(lang);
        if (goal && goal.kind === "attempts") {
          const days = await api.activity(1, -new Date().getTimezoneOffset());
          const done = days[0]?.attempts ?? 0;
          if (done < goal.target) size = Math.min(40, Math.max(1, goal.target - done));
        }
      } catch {
        // 读目标失败就按默认题量出
      }
      const s = await api.startSession(lang, size, Date.now() >>> 0, mode);
      // 兼容旧后端（返回裸数组）与半新不旧的 dev 缓存，别让整个页面白掉
      setSession(Array.isArray(s) ? { exercises: s, review: 0 } : s);
      setIdx(0);
      reset();
    } catch (e) {
      setError(String(e));
    }
  }, [lang, mode, reset]);

  useEffect(() => {
    load();
  }, [load]);

  const submit = async (answer: Answer) => {
    if (!ex) return;
    try {
      setResult(await api.submit(lang, ex, answer, Date.now() - started.current));
      onFinished?.();
    } catch (e) {
      setError(String(e));
    }
  };

  const advance = () => {
    if (!session) return;
    if (idx + 1 >= session.exercises.length) {
      setIdx(session.exercises.length);
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

  if (session.exercises.length === 0) {
    const why: Record<string, string> = {
      article:
        "阅读需要文章。到词库页用「导入文章」粘一篇（标题、正文、题目），或往 pack 里加 articles.json。",
      sentence:
        "拼句需要 3 个词以上的句子。在词库页导入整句或短语，或往 pack 的 sentences.json 里加。",
      audio:
        "听音题要有可选项。先在词库导入一批词；没有预生成音频也会按需合成，只要设置里有可用音色。",
      recall: "这门语言还没有可背的词条。导入单词书或自定义词条即可。",
      weak: "近 30 天没有错题，或者错题对应的词条已经不在包里。",
    };
    return (
      <div className="panel">
        <p className="muted">{why[mode] ?? "这种练法在当前语言包里还没有素材。"}</p>
        <button onClick={onExit}>换一种</button>
      </div>
    );
  }

  if (idx >= session.exercises.length) {
    return (
      <div className="panel done">
        <div className="done-mark">✓</div>
        <h2>本组完成</h2>
        <p className="muted">
          共 {session.exercises.length} 题{session.review > 0 && ` · 其中复习 ${session.review} 题`}
        </p>
        <div className="actions center">
          <button className="ghost" onClick={onExit}>
            换一种练法
          </button>
          <button className="primary" onClick={load}>
            再来一组
          </button>
        </div>
      </div>
    );
  }

  if (!ex) return <div className="panel muted">本组已结束。</div>;

  const ready =
    ex.kind === "build-sentence" ? placed.length === ex.chunks.length : picked !== null;

  const progress = ((idx + (result ? 1 : 0)) / session.exercises.length) * 100;

  return (
    <div className="panel practice">
      <div className="progress">
        <button className="ghost back" onClick={onExit} title="返回选择">
          ←
        </button>
        <div className="bar">
          <div className="fill" style={{ width: `${progress}%` }} />
        </div>
        <span className="muted">
          {idx + 1} / {session.exercises.length} · {kindLabel(ex)}
          {session.review > 0 && ` · 复习 ${session.review}`}
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
        onRecall={(rating) => submit({ kind: "recall", rating })}
      />

      {result ? (
        <Feedback result={result} ex={ex} onNext={advance} lang={lang} />
      ) : ex.kind === "recall" ? null : (
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
    case "word-from-audio":
      return "听音选词";
    case "article-mcq":
      return "阅读理解";
    case "recall":
      return "背诵卡";
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
  onRecall,
}: {
  lang: string;
  ex: Exercise;
  placed: number[];
  setPlaced: (v: number[]) => void;
  picked: number | null;
  setPicked: (v: number | null) => void;
  locked: boolean;
  onRecall: (rating: number) => void;
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

  if (ex.kind === "word-from-audio") {
    const answer = ex.options[ex.answer];
    return (
      <div className="exercise">
        <PlayButton lang={lang} text={answer} entryId={ex.entry} big />
        <Options options={ex.options} picked={picked} setPicked={setPicked} locked={locked} />
      </div>
    );
  }

  if (ex.kind === "recall") {
    return <RecallCard lang={lang} ex={ex} onSubmit={onRecall} locked={locked} />;
  }

  if (ex.kind === "chat-scenario") {
    return <p className="muted">情景题在「情景」页练习。</p>;
  }

  return (
    <ArticleQuestion
      lang={lang}
      ex={ex}
      picked={picked}
      setPicked={setPicked}
      locked={locked}
    />
  );
}

/// 阅读理解：先出正文，再出题。正文与题目分开取，切题时不必重下整篇文章。
function ArticleQuestion({
  lang,
  ex,
  picked,
  setPicked,
  locked,
}: {
  lang: string;
  ex: Extract<Exercise, { kind: "article-mcq" }>;
  picked: number | null;
  setPicked: (v: number | null) => void;
  locked: boolean;
}) {
  const [article, setArticle] = useState<Article | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setArticle(null);
    setError(null);
    api
      .getArticle(lang, ex.article)
      .then((a) => alive && setArticle(a))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [lang, ex.article]);

  if (error) {
    return (
      <div className="exercise">
        <p className="error">取正文失败：{error}</p>
      </div>
    );
  }

  if (!article) return <div className="exercise muted">正在取正文…</div>;

  return (
    <div className="exercise article">
      <article className="passage">
        <h3>{article.title}</h3>
        {article.body.split(/\n{2,}/).map((para, i) => (
          <p key={i}>{para}</p>
        ))}
      </article>

      <div className="question-block">
        <p className="question">
          <span className="tag">第 {ex.question + 1} 题</span> {ex.prompt}
        </p>
        <Options options={ex.options} picked={picked} setPicked={setPicked} locked={locked} />
      </div>
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

/// 背诵卡：正面出词，直接自评；选完才给释义。
///
/// 先看释义再自评等于给自己答案，评出来的「认识」不可信。顺序反过来，
/// 用户先凭记忆判断，再对答案。
function RecallCard({
  lang,
  ex,
  onSubmit,
  locked,
}: {
  lang: string;
  ex: Extract<Exercise, { kind: "recall" }>;
  onSubmit: (rating: number) => void;
  locked: boolean;
}) {
  return (
    <div className="exercise recall">
      <div className="recall-front">
        <PlayButton lang={lang} text={ex.surface} entryId={ex.entry} />
        <h2>{ex.surface}</h2>
        {ex.reading && <p className="muted">/{ex.reading}/</p>}
      </div>

      <p className="muted recall-ask">认识这个词吗？</p>

      <div className="rating">
        {[
          { r: 3, label: "认识", cls: "good" },
          { r: 2, label: "模糊", cls: "hard" },
          { r: 1, label: "不认识", cls: "again" },
          { r: 4, label: "太简单", cls: "easy" },
        ].map((x) => (
          <button key={x.r} className={x.cls} disabled={locked} onClick={() => onSubmit(x.r)}>
            {x.label}
          </button>
        ))}
      </div>
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
  ex,
  onNext,
  lang,
}: {
  result: SubmitResult;
  ex: Exercise;
  onNext: () => void;
  lang: string;
}) {
  const { grade, card } = result;
  const days = card ? Math.max(0, Math.round((card.state.due * 1000 - Date.now()) / 86_400_000)) : null;
  const recall = ex.kind === "recall" ? ex : null;
  const expected = recall ? undefined : grade.items.find((i) => !i.correct && i.expected)?.expected;

  const RATING_LABEL: Record<number, string> = {
    1: "不认识",
    2: "模糊",
    3: "认识",
    4: "太简单",
  };

  return (
    <div className={`feedback ${grade.correct ? "ok" : "bad"}`}>
      <div className="feedback-head">
        <p className="verdict">
          {recall
            ? `自评：${RATING_LABEL[result.rating] ?? "已记录"}`
            : grade.correct
              ? "正确"
              : grade.ratio > 0
                ? `部分正确 ${Math.round(grade.ratio * 100)}%`
                : "再想想"}
        </p>
        {expected && (
          <span className="answer-reveal">
            应为 <b>{expected}</b>
            <PlayButton lang={lang} text={expected} />
          </span>
        )}
      </div>

      {recall && (
        <p className="recall-back">
          <b>{recall.surface}</b> · {recall.gloss ?? "（这个词还没有释义）"}
        </p>
      )}

      {!recall && !grade.correct && (
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