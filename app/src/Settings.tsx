import { useEffect, useState } from "react";
import { api, type LlmSettings, type ProviderPreset, type VoiceInfo, type VoiceOption } from "./api";
import { playUrl } from "./audio";

const SAMPLE: Record<string, string> = {
  en: "Hi there. This is how I sound when I read a sentence.",
  ko: "안녕하세요. 제 목소리는 이렇게 들려요.",
};

export function Settings({
  lang,
  onClose,
  onDataReset,
}: {
  lang: string | null;
  onClose: () => void;
  onDataReset?: () => void;
}) {
  const [voices, setVoices] = useState<VoiceInfo[]>([]);
  const [voice, setVoice] = useState("");
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [llm, setLlm] = useState<LlmSettings | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [providers, setProviders] = useState<ProviderPreset[]>([]);
  const [provider, setProvider] = useState("custom");
  const [models, setModels] = useState<string[]>([]);
  const [checked, setChecked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmScope, setConfirmScope] = useState<"progress" | "all" | null>(null);
  const [showCatalog, setShowCatalog] = useState(false);
  const [catalog, setCatalog] = useState<VoiceOption[] | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [backingUp, setBackingUp] = useState(false);

  // 端点一旦确认过，模型才以下拉框出现；没 key 时不要凭空显示模型列表。
  useEffect(() => {
    let alive = true;
    (async () => {
      const [presets, saved] = await Promise.all([
        api.llmProviders().catch(() => [] as ProviderPreset[]),
        api.getLlmSettings().catch(() => null),
      ]);
      if (!alive) return;
      setProviders(presets);
      if (!saved) return;
      setLlm(saved);
      setBaseUrl(saved.base_url);
      setModel(saved.model);
      const hit = presets.find((p) => p.base_url === saved.base_url);
      if (hit) setProvider(hit.id);
      if (!saved.has_key) return;

      setBusy(true);
      try {
        const list = await api.llmModels(saved.base_url);
        if (!alive) return;
        setModels(list);
        setChecked(true);
        if (list.length > 0 && !list.includes(saved.model)) setModel(list[0]);
      } catch {
        // 存过 key 但端点暂时不通，等用户手动确认
      } finally {
        if (alive) setBusy(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    if (!lang) return;
    api.ttsVoices(lang).then((v) => alive && setVoices(v)).catch(() => {});
    api.getVoice(lang).then((v) => alive && setVoice(v ?? "")).catch(() => {});
    setCatalog(null);
    setShowCatalog(false);
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

  /// 展开音色目录时惰性拉一次索引。
  useEffect(() => {
    if (!showCatalog || !lang || catalog) return;
    let alive = true;
    api
      .voiceCatalog(lang)
      .then((list) => alive && setCatalog(list))
      .catch((e) => alive && setError(`读取音色索引失败：${e}`));
    return () => {
      alive = false;
    };
  }, [showCatalog, lang, catalog]);

  const downloadVoice = async (id: string) => {
    if (!lang) return;
    setDownloading(id);
    setError(null);
    setNote(null);
    try {
      await api.installVoice(id);
      setCatalog((c) => c?.map((v) => (v.id === id ? { ...v, installed: true } : v)) ?? c);
      setVoices(await api.ttsVoices(lang));
      setNote("音色已下载，可在上面选中");
    } catch (e) {
      setError(`下载失败：${e}`);
    } finally {
      setDownloading(null);
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

  /// 确认端点：能用这把 key 列出模型，才算配置可用。
  const confirmEndpoint = async () => {
    setBusy(true);
    setError(null);
    setNote(null);
    setChecked(false);
    try {
      const list = await api.llmModels(baseUrl, apiKey || undefined);
      setModels(list);
      setChecked(true);
      if (list.length === 0) {
        setError("端点连通，但没有返回模型");
        return;
      }
      setNote(`端点可用 · ${list.length} 个模型`);
      if (!list.includes(model)) setModel(list[0]);
    } catch (e) {
      setError(`端点确认失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  /// 清数据。progress 只清学习记录，all 连导入内容与设置一起清。
  const doReset = async (scope: "progress" | "all") => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await api.resetData(scope);
      onDataReset?.();
      if (scope === "all") {
        onClose();
        return;
      }
      setNote("学习记录已清除");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setConfirmScope(null);
    }
  };

  const saveLlm = async () => {
    setError(null);
    if (!model.trim()) {
      setError("先确认端点并选一个模型");
      return;
    }
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

          <div className="voice-store">
            <button className="ghost small" onClick={() => setShowCatalog((v) => !v)}>
              {showCatalog ? "收起可下载音色" : "更多音色（下载）"}
            </button>
            {showCatalog && (
              <>
                {!catalog && <p className="muted">读取音色索引…</p>}
                {catalog && catalog.filter((v) => !v.installed).length === 0 && (
                  <p className="muted">这门语言的音色都已下载。</p>
                )}
                {catalog
                  ?.filter((v) => !v.installed)
                  .map((v) => (
                    <div className="catalog-row" key={v.id}>
                      <div>
                        <b>{v.name}</b>
                        <span className="muted">
                          {" "}
                          {v.quality}
                          {v.size_mb > 0 ? ` · 约 ${v.size_mb} MB` : ""}
                        </span>
                      </div>
                      <button
                        className="small"
                        disabled={downloading === v.id}
                        onClick={() => downloadVoice(v.id)}
                      >
                        {downloading === v.id ? "下载中…" : "下载"}
                      </button>
                    </div>
                  ))}
              </>
            )}
          </div>
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
                  setChecked(false);
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
            <div className="model-row">
              <input
                value={baseUrl}
                onChange={(e) => {
                  setBaseUrl(e.target.value);
                  setModels([]);
                  setChecked(false);
                }}
                placeholder="https://api.openai.com/v1"
              />
              <button className="small" onClick={confirmEndpoint} disabled={!baseUrl || busy}>
                {busy ? "…" : "确认端点"}
              </button>
            </div>
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

          {models.length > 0 ? (
            <label>
              模型
              <select value={model} onChange={(e) => setModel(e.target.value)}>
                {!models.includes(model) && model && <option value={model}>{model}（当前）</option>}
                {models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <p className="muted">
              {checked
                ? "这个端点没有返回模型，检查 key 与 base URL。"
                : "填好 key 后点「确认端点」，能列出的模型会以下拉框出现在这里。"}
            </p>
          )}

          <div className="actions">
            <button className="primary" onClick={saveLlm}>
              保存
            </button>
          </div>
        </section>

        <section>
          <p className="section-label">数据</p>
          <p className="muted">
            导出学习进度：把当前语言的卡片导成 CSV（词、释义、题型、熟练度、到期日），
            可直接导进表格或 Anki 备份。
          </p>
          <div className="actions">
            <button
              className="small"
              disabled={!lang || exporting}
              onClick={async () => {
                if (!lang) return;
                setExporting(true);
                setError(null);
                setNote(null);
                try {
                  const path = await api.exportCards(lang);
                  setNote(`已导出：${path}`);
                } catch (e) {
                  setError(`导出失败：${e}`);
                } finally {
                  setExporting(false);
                }
              }}
            >
              {exporting ? "导出中…" : "导出 CSV"}
            </button>
            <button
              className="small"
              disabled={backingUp}
              onClick={async () => {
                setBackingUp(true);
                setError(null);
                setNote(null);
                try {
                  const path = await api.backupDb();
                  setNote(`已备份整库：${path}`);
                } catch (e) {
                  setError(`备份失败：${e}`);
                } finally {
                  setBackingUp(false);
                }
              }}
            >
              {backingUp ? "备份中…" : "备份整库"}
            </button>
          </div>
          <p className="muted">
            清除学习记录：卡片、复习进度、作答、番茄钟、聊天。导入的词书与自定义词条保留。
          </p>
          <p className="muted">
            清空所有数据：连导入的词条、文章和设置一起删，回到刚装好的状态。
          </p>
          <div className="actions">
            <button className="small" onClick={() => setConfirmScope("progress")}>
              清除学习记录
            </button>
            <button className="small danger" onClick={() => setConfirmScope("all")}>
              清空所有数据
            </button>
          </div>

          {confirmScope && (
            <div className="confirm-box">
              <p className="muted">
                {confirmScope === "progress"
                  ? "确定清除学习记录？这一步不能撤销。"
                  : "确定清空所有数据？卡片、导入内容、文章与设置都会删除。"}
              </p>
              <div className="actions">
                <button className="ghost" onClick={() => setConfirmScope(null)} disabled={busy}>
                  取消
                </button>
                <button
                  className="primary"
                  onClick={() => doReset(confirmScope)}
                  disabled={busy}
                >
                  {busy ? "处理中…" : "确定清除"}
                </button>
              </div>
            </div>
          )}
        </section>

        {note && <p className="muted">{note}</p>}
        {error && <p className="error">{error}</p>}
      </div>
    </div>
  );
}