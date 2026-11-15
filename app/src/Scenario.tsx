import { useEffect, useState } from "react";
import {
  api,
  type CardView,
  type Dialog,
  type DialogNode,
  type DialogSummary,
  type Entry,
  type Speaker,
  type Token,
} from "./api";

type Bubble = {
  speaker: Speaker;
  text: string;
  tokens?: Token[];
  chosen?: boolean;
  awkward?: boolean;
};

const bubbleOf = (n: DialogNode): Bubble => ({
  speaker: n.speaker,
  text: n.text,
  tokens: n.tokens,
});

export function Scenario({ lang }: { lang: string }) {
  const [list, setList] = useState<DialogSummary[]>([]);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [nodeId, setNodeId] = useState<string | null>(null);
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [word, setWord] = useState<{ token: Token; entry: Entry | null } | null>(null);
  const [card, setCard] = useState<CardView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDialog(null);
    setBubbles([]);
    api
      .listDialogs(lang)
      .then(setList)
      .catch((e) => setError(String(e)));
  }, [lang]);

  const open = async (id: string) => {
    try {
      const d = await api.getDialog(lang, id);
      const start = d.nodes.find((n) => n.id === d.start);
      setDialog(d);
      setCard(null);
      setWord(null);
      setBubbles(start ? [bubbleOf(start)] : []);
      setNodeId(start?.id ?? null);
    } catch (e) {
      setError(String(e));
    }
  };

  const node = dialog?.nodes.find((n) => n.id === nodeId) ?? null;

  const goNext = () => {
    if (!dialog || !node?.next) return;
    const next = dialog.nodes.find((n) => n.id === node.next);
    if (!next) return;
    setBubbles((b) => [...b, bubbleOf(next)]);
    setNodeId(next.id);
  };

  const choose = (text: string, nextId: string, awkward?: boolean) => {
    if (!dialog) return;
    const next = dialog.nodes.find((n) => n.id === nextId);
    setBubbles((b) => {
      const out: Bubble[] = [...b, { speaker: "b", text, chosen: true, awkward }];
      if (next) out.push(bubbleOf(next));
      return out;
    });
    setNodeId(nextId);
  };

  const inspect = async (token: Token) => {
    try {
      const entry = await api.define(lang, token.lemma ?? token.surface);
      setWord({ token, entry });
      setCard(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const addToReview = async () => {
    if (!word?.entry) return;
    try {
      setCard(await api.addCard(lang, word.entry.id));
    } catch (e) {
      setError(String(e));
    }
  };

  if (error) return <div className="panel error">出错了：{error}</div>;

  if (!dialog) {
    if (list.length === 0) {
      return <div className="panel muted">这个语言包还没有情景对话。</div>;
    }
    return (
      <div className="panel">
        <ul className="dialog-list">
          {list.map((d) => (
            <li key={d.id}>
              <button onClick={() => open(d.id)}>
                <b>{d.title}</b>
                <span className="muted">
                  {d.theme} · {d.nodes} 条消息
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  return (
    <div className="panel scenario">
      <div className="scenario-head">
        <button className="ghost" onClick={() => setDialog(null)}>
          ← 返回
        </button>
        <span>{dialog.title}</span>
      </div>

      <div className="chat">
        {bubbles.map((b, i) => (
          <div key={i} className={`bubble ${b.speaker}${b.chosen ? " chosen" : ""}`}>
            {b.speaker === "narrator" ? (
              <em className="muted">{b.text}</em>
            ) : b.tokens?.length ? (
              <TokenLine tokens={b.tokens} onPick={inspect} />
            ) : (
              <span>{b.text}</span>
            )}
            {b.awkward && <span className="awkward">不太合适</span>}
          </div>
        ))}
      </div>

      {node && (
        <div className="replies">
          {node.choices?.length ? (
            node.choices.map((c) => (
              <button key={c.next + c.text} onClick={() => choose(c.text, c.next, c.awkward)}>
                {c.text}
              </button>
            ))
          ) : node.next ? (
            <button className="primary" onClick={goNext}>
              继续
            </button>
          ) : (
            <span className="muted">对话结束 · 点任意词可查义并加入复习</span>
          )}
        </div>
      )}

      {word && (
        <div className="word-panel">
          <div>
            <b>{word.token.surface}</b>
            {word.token.reading && <span className="muted"> /{word.token.reading}/</span>}
            {word.token.pos && <span className="tag">{word.token.pos}</span>}
          </div>
          <div className="muted">{word.entry?.gloss ?? "词库里还没有这个词的释义"}</div>
          {word.entry && (
            <button
              className="small"
              disabled={!!card}
              onClick={addToReview}
            >
              {card ? "已加入复习" : "加入复习"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function TokenLine({ tokens, onPick }: { tokens: Token[]; onPick: (t: Token) => void }) {
  return (
    <span className="tokens">
      {tokens.map((t, i) => {
        const space = i > 0 ? " " : "";
        if (!/[\p{L}\p{N}]/u.test(t.surface)) {
          return <span key={i}>{t.surface}</span>;
        }
        return (
          <span key={i}>
            {space}
            <button className="tok" onClick={() => onPick(t)}>
              {t.surface}
            </button>
          </span>
        );
      })}
    </span>
  );
}