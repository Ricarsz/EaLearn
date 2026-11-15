import { useEffect, useState } from "react";
import { api, type LlmSettings, type VoiceInfo } from "./api";
import { playUrl } from "./audio";

const SAMPLE: Record<string, string> = {
  en: "Hi there. This is how I sound when I read a sentence.",
  ko: "안녕하세요. 제 목소리는 이렇게 들려요.",
};

export function Settings({ lang, onClose }: { lang: string | null; onClose: () => void }) {
  const [voices, setVoices] = useState<VoiceInfo[]>([]);
  const [voice, setVoice] = useState("");
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [llm, setLlm] = useState<LlmSettings | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .getLlmSettings()
      .then((s) => {
        if (!alive) return;
        setLlm(s);
        setBaseUrl(s.base_url);
        setModel(s.model);
      })
      .catch((e) => alive && setError(String(e)));
    if (!lang) return;
    api.ttsVoices(lang).then((v) => alive && setVoices(v)).catch(() => {});
    api.getVoice(lang).then((v) => alive && setVoice(v ?? "")).catch(() => {});
    return () => {
      alive = false;
    };
  }, [lang]);

  const pickVoice = async (v: string) => {
    setVoice(v);
    if (lang && v) {
      await api.setVoice(lang, v).catch((e) => setError(String(e)));
      setNote("已保存音色");
    }
  };

  const preview = async (id: string) => {
    if (!lang) return;
    setPreviewing(id);
    setError(null);
    try {
      const url = await api.speak(lang, SAMPLE[lang] ?? "Hello.", undefined, id);
      await playUrl(url);
    } catch (e) {
      setError(`试听失败：${e}`);
    } finally {
      setPreviewing(null);
    }
  };

  const saveLlm = async () => {
    setError(null);
    try {
      await api.setLlmSettings(baseUrl, model, apiKey || undefined);
      setApiKey("");
      setLlm(await api.getLlmSettings());
      setNote("已保存模型配置");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-head">
          <h2>设置</h2>
          <button className="ghost" onClick={onClose}>
            关闭
          </button>
        </header>

        <section>
          <p className="section-label">发音</p>
          {!lang && <p className="muted">先选择一门语言</p>}
          {lang && voices.length === 0 && <p className="muted">没有可用音色</p>}
          <ul className="voice-list">
            {voices.map((v) => (
              <li key={v.id} className={voice === v.id ? "on" : ""}>
                <button className="voice-pick" onClick={() => pickVoice(v.id)} disabled={!v.ready}>
                  <span className="radio" />
                  <span>{v.label}</span>
                  <span className="muted">{v.engine}</span>
                </button>
                <button
                  className="play"
                  onClick={() => preview(v.id)}
                  title="试听"
                  disabled={!v.ready}
                >
                  {previewing === v.id ? "…" : "▶"}
                </button>
              </li>
            ))}
          </ul>
          <p className="muted">留空表示跟随语言包自带的音色。</p>
        </section>

        <section>
          <p className="section-label">模型（自由聊）</p>
          <label>
            Base URL
            <input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
            />
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
              placeholder={llm?.has_key ? `已保存 ${llm.key_hint}` : "sk-…"}
            />
          </label>
          <div className="actions">
            <button className="primary" onClick={saveLlm}>
              保存
            </button>
          </div>
        </section>

        {note && <p className="muted">{note}</p>}
        {error && <p className="error">{error}</p>}
      </div>
    </div>
  );
}