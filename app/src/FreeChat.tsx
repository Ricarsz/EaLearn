import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type LlmSettings, type TutorTurn } from "./api";
import { THEMES } from "./themes";

type Msg = { role: "user" | "tutor"; text: string; turn?: TutorTurn };

export function FreeChat({ lang }: { lang: string }) {
  const session = useRef(`chat:${lang}`);
  const [settings, setSettings] = useState<LlmSettings | null>(null);
  const [editing, setEditing] = useState(false);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [theme, setTheme] = useState("gossip");
  const [level, setLevel] = useState(2);
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    api
      .getLlmSettings()
      .then((s) => {
        if (!alive) return;
        setSettings(s);
        setBaseUrl(s.base_url);
        setModel(s.model);
        setEditing(!s.has_key);
      })
      .catch((e) => alive && setError(String(e)));

    api
      .chatHistory(session.current, 20)
      .then((rows) => {
        if (!alive || rows.length === 0) return;
        setMsgs(rows.map(([role, text]) => ({ role: role === "user" ? "user" : "tutor", text })));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [lang]);

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [msgs]);

  const saveSettings = async () => {
    try {
      await api.setLlmSettings(baseUrl, model, apiKey || undefined);
      setApiKey("");
      setEditing(false);
      setSettings(await api.getLlmSettings());
    } catch (e) {
      setError(String(e));
    }
  };

  const send = async () => {
    const text = draft.trim();
    if (!text || busy) return;
    setError(null);
    setBusy(true);
    setDraft("");
    setMsgs((m) => [...m, { role: "user", text }, { role: "tutor", text: "" }]);

    const unlisten = await listen<{ session: string; text: string }>("llm://delta", (e) => {
      if (e.payload.session !== session.current) return;
      setMsgs((m) => {
        const copy = [...m];
        const last = copy[copy.length - 1];
        if (!last || last.role !== "tutor") return m;
        copy[copy.length - 1] = { ...last, text: last.text + e.payload.text };
        return copy;
      });
    });

    try {
      const t = THEMES.find((x) => x.id === theme);
      const turn = await api.chatSend(
        lang,
        session.current,
        t ? `${t.label}（${t.hint}）` : theme,
        level,
        text,
      );
      setMsgs((m) => {
        const copy = [...m];
        const last = copy[copy.length - 1];
        if (last?.role === "tutor") copy[copy.length - 1] = { role: "tutor", text: turn.reply, turn };
        return copy;
      });
    } catch (e) {
      setError(String(e));
      // 去掉那个空的助教气泡
      setMsgs((m) => (m[m.length - 1]?.role === "tutor" && !m[m.length - 1].text ? m.slice(0, -1) : m));
    } finally {
      unlisten();
      setBusy(false);
    }
  };

  return (
    <div className="freechat">
      <div className="chat-toolbar">
        <label title={THEMES.find((t) => t.id === theme)?.hint}>
          话题
          <select value={theme} onChange={(e) => setTheme(e.target.value)}>
            {THEMES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          难度
          <select value={level} onChange={(e) => setLevel(Number(e.target.value))}>
            {[1, 2, 3, 4, 5].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
        <span className="muted">
          {settings?.has_key ? `${settings.model}${settings.key_hint}` : "未配置 key"}
        </span>
        <button className="ghost" onClick={() => setEditing((v) => !v)}>
          设置
        </button>
      </div>

      {editing && (
        <div className="settings-form">
          <label>
            Base URL
            <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.openai.com/v1" />
          </label>
          <label>
            模型
            <input value={model} onChange={(e) => setModel(e.target.value)} placeholder="gpt-4o-mini" />
          </label>
          <label>
            API Key
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={settings?.has_key ? "已保存，留空则不修改" : "sk-…"}
            />
          </label>
          <div className="actions">
            <button className="primary" onClick={saveSettings}>
              保存
            </button>
          </div>
        </div>
      )}

      <div className="chat" ref={scroller}>
        {msgs.length === 0 && (
          <p className="muted">
            用{lang === "ko" ? "韩语" : "英语"}随便聊点什么。对方说的话会带中文翻译，点标注可以看用法。
          </p>
        )}
        {msgs.map((m, i) => (
          <div key={i} className={`bubble ${m.role === "user" ? "b" : "a"}`}>
            <span>{m.text || (busy && i === msgs.length - 1 ? "…" : "")}</span>
            {m.turn?.translation && <p className="translation">{m.turn.translation}</p>}
            {m.turn?.correction && (
              <p className="correction-line">
                更自然的说法：<b>{m.turn.correction.fixed}</b>
                {m.turn.correction.note && <span className="muted"> · {m.turn.correction.note}</span>}
              </p>
            )}
            {!!m.turn?.annotations?.length && (
              <div className="annot">
                {m.turn.annotations.map((a, k) => (
                  <span key={k} className="annot-item" title={a.note ?? a.lemma ?? ""}>
                    <b>{a.surface}</b>
                    {a.gloss && <span className="muted"> {a.gloss}</span>}
                    {a.kind && <em className="kind">{a.kind}</em>}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>

      {error && <p className="error">{error}</p>}

      <div className="composer">
        <textarea
          value={draft}
          placeholder="说点什么…（Enter 发送，Shift+Enter 换行）"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        />
        <button className="primary" disabled={busy || !draft.trim()} onClick={send}>
          {busy ? "…" : "发送"}
        </button>
      </div>
    </div>
  );
}