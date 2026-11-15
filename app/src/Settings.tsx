import { useEffect, useState } from "react";
import { api, type LlmSettings, type ProviderPreset, type VoiceInfo } from "./api";
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
  const [providers, setProviders] = useState<ProviderPreset[]>([]);
  const [provider, setProvider] = useState("custom");
  const [models, setModels] = useState<string[]>([]);
  const [fetching, setFetching] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api.llmProviders().then((p) => alive && setProviders(p)).catch(() => {});
    api
      .getLlmSettings()
      .then((s) => {
        if (!alive) return;
        setLlm(s);
        setBaseUrl(s.base_url);
        setModel(s.model);
        const hit = providers.find((p) => p.base_url === s.base_url);
        if (hit) setProvider(hit.id);
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

  const fetchModels = async () => {
    setFetching(true);
    setError(null);
    try {
      const list = await api.llmModels(baseUrl, apiKey || undefined);
      setModels(list);
      if (list.length === 0) setError("接口没有返回可用模型");
      setNote(`拿到 ${list.length} 个模型`);
    } catch (e) {
      setError(`拉取模型失败：${e}`);
    } finally {
      setFetching(false);
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
            供应商
            <select
              value={provider}
              onChange={(e) => {
                const id = e.target.value;
                setProvider(id);
                const p = providers.find((x) => x.id === id);
                if (p) {
                  setBaseUrl(p.base_url);
                  setModels([]);
                }
              }}
            >
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          {providers.find((p) => p.id === provider)?.note && (
            <p className="muted">{providers.find((p) => p.id === provider)?.note}</p>
          )}
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
            <div className="model-row">
              <input
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="deepseek-chat"
                list="model-options"
              />
              <button className="small" onClick={fetchModels} disabled={!baseUrl || fetching}>
                {fetching ? "…" : "拉取"}
              </button>
            </div>
          </label>
          <datalist id="model-options">
            {models.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
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