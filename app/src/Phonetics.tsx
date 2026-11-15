import { useState } from "react";
import { api } from "./api";
import { playUrl } from "./audio";
import {
  EN_CONSONANTS,
  EN_DIPHTHONGS,
  EN_VOWELS,
  KO_CONSONANTS,
  KO_FINALS,
  KO_VOWELS,
  type Jamo,
  type Phoneme,
} from "./phonemes";

/// 试听一个词/音节。没装音色就静默失败，不打断阅读。
function Speak({ lang, text, label }: { lang: string; text: string; label?: string }) {
  const [busy, setBusy] = useState(false);
  return (
    <button
      className={label ? "say-btn" : "play small-play"}
      title={`朗读 ${text}`}
      onClick={async () => {
        if (busy) return;
        setBusy(true);
        try {
          await playUrl(await api.speak(lang, text));
        } catch {
          /* 音色不可用时保持安静 */
        } finally {
          setBusy(false);
        }
      }}
    >
      {busy ? "…" : (label ?? "▶")}
    </button>
  );
}

function PhonemeGrid({ rows, lang }: { rows: Phoneme[]; lang: string }) {
  return (
    <div className="phoneme-grid">
      {rows.map((r) => (
        <div className="phoneme-cell" key={`${r.symbol}-${r.example}`}>
          <b>/{r.symbol}/</b>
          <span>{r.example}</span>
          <span className="muted">{r.hint}</span>
          <Speak lang={lang} text={r.example} />
        </div>
      ))}
    </div>
  );
}

function JamoTable({ rows, lang }: { rows: Jamo[]; lang: string }) {
  return (
    <table className="jamo-table">
      <thead>
        <tr>
          <th>字母</th>
          <th>名称</th>
          <th>罗马字</th>
          <th>例</th>
          <th>试听</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.letter}>
            <td className="jamo-letter">{r.letter}</td>
            <td className="muted">{r.name}</td>
            <td>{r.roman}</td>
            <td>{r.example}</td>
            <td className="row-actions">
              {/* 字母单独念：辅音是 ㄱ+ㅡ = 그，元音是 ㅇ+元音 = 아 */}
              <Speak lang={lang} text={r.sound} label="字音" />
              {r.example !== r.sound && <Speak lang={lang} text={r.example} label="例词" />}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function Phonetics({ lang }: { lang: string }) {
  const [which, setWhich] = useState<"en" | "ko">(lang === "ko" ? "ko" : "en");

  return (
    <div className="panel wide phonetics">
      <div className="mode-switch">
        <button className={which === "en" ? "active" : ""} onClick={() => setWhich("en")}>
          英语音标
        </button>
        <button className={which === "ko" ? "active" : ""} onClick={() => setWhich("ko")}>
          韩语字母
        </button>
      </div>

      {which === "en" ? (
        <>
          <section>
            <p className="section-label">单元音</p>
            <PhonemeGrid rows={EN_VOWELS} lang="en" />
          </section>
          <section>
            <p className="section-label">双元音</p>
            <PhonemeGrid rows={EN_DIPHTHONGS} lang="en" />
          </section>
          <section>
            <p className="section-label">辅音</p>
            <PhonemeGrid rows={EN_CONSONANTS} lang="en" />
          </section>
        </>
      ) : (
        <>
          <section>
            <p className="section-label">辅音（19）</p>
            <JamoTable rows={KO_CONSONANTS} lang="ko" />
          </section>
          <section>
            <p className="section-label">元音（21）</p>
            <JamoTable rows={KO_VOWELS} lang="ko" />
          </section>
          <section>
            <p className="section-label">받침 代表音（7）</p>
            <table className="jamo-table">
              <thead>
                <tr>
                  <th>收音写法</th>
                  <th>实际读音</th>
                  <th>例</th>
                  <th>释义</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {KO_FINALS.map((r) => (
                  <tr key={r.sound}>
                    <td className="jamo-letter">{r.letter}</td>
                    <td>{r.sound}</td>
                    <td>{r.example}</td>
                    <td className="muted">{r.gloss}</td>
                    <td>
                      <Speak lang="ko" text={r.example} label="例词" />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </>
      )}

      <p className="muted">
        表只做查询参考，不参与判分。点 ▶ 用当前音色朗读示例；连读与音变（연음、비음화 等）按实际发音再变。
      </p>
    </div>
  );
}