import { invoke } from "@tauri-apps/api/core";

export type Token = {
  surface: string;
  lemma?: string;
  reading?: string;
  pos?: string;
  gloss?: string;
  span: [number, number];
};

export type Coverage = {
  buildSentence: number;
  wordFromImage: number;
  wordFromAudio: number;
  articleMcq: number;
};

export type PackSummary = {
  lang: string;
  display: string;
  version: string;
  enabled: boolean;
  words: number;
  sentences: number;
  articles: number;
  dialogs: number;
  coverage: Coverage;
};

export type Exercise =
  | {
      kind: "build-sentence";
      entry: string;
      chunks: string[];
      answer: number[];
      prompt?: string;
    }
  | { kind: "word-from-image"; entry: string; image: string; options: string[]; answer: number }
  | { kind: "word-from-audio"; entry: string; audio: string; options: string[]; answer: number }
  | {
      kind: "article-mcq";
      article: string;
      question: number;
      prompt: string;
      options: string[];
      answer: number;
    }
  | { kind: "chat-scenario"; dialog: string };

export type Answer =
  | { kind: "order"; order: number[] }
  | { kind: "choice"; index: number };

export type GradeItem = {
  index: number;
  correct: boolean;
  expected?: string;
  note?: string;
};

export type Grade = { correct: boolean; ratio: number; items: GradeItem[] };

export type CardState = {
  stability: number;
  difficulty: number;
  reps: number;
  lapses: number;
  last_review?: number;
  due: number;
};

export type CardView = {
  card_id: number;
  kind: "word" | "sentence" | "slang" | "grammar";
  state: CardState;
  label: string;
};

export type SubmitResult = { grade: Grade; rating: number; card: CardView | null };

export type Stats = {
  cards: number;
  due_now: number;
  reviews_today: number;
  attempts_today: number;
  accuracy_today: number;
};

export type Speaker = "a" | "b" | "narrator";

export type Choice = { text: string; next: string; reply?: string; awkward: boolean };

export type DialogNode = {
  id: string;
  speaker: Speaker;
  text: string;
  tokens?: Token[];
  next?: string;
  choices?: Choice[];
};

export type Dialog = {
  id: string;
  title: string;
  theme: string;
  start: string;
  nodes: DialogNode[];
  tags?: string[];
};

export type Entry = {
  id: string;
  surface: string;
  lemma?: string;
  gloss?: string;
  tags?: string[];
  tokens?: Token[];
  audio?: string;
  image?: string;
  difficulty?: number;
};

export type Annotation = {
  surface: string;
  lemma?: string;
  kind?: string;
  gloss?: string;
  note?: string;
};

export type Correction = { original: string; fixed: string; note?: string };

export type TutorTurn = {
  reply: string;
  translation?: string;
  annotations?: Annotation[];
  correction?: Correction;
  difficulty?: number;
  warnings?: string[];
};

export type LlmSettings = {
  base_url: string;
  model: string;
  has_key: boolean;
  key_hint: string;
};

export type VoiceInfo = { id: string; label: string; engine: string; ready: boolean };

export type DialogSummary = { id: string; title: string; theme: string; nodes: number };

export const api = {
  listPacks: () => invoke<PackSummary[]>("list_packs"),
  startSession: (lang: string, limit: number, seed: number) =>
    invoke<Exercise[]>("start_session", { lang, limit, seed }),
  submit: (lang: string, exercise: Exercise, answer: Answer, elapsedMs: number) =>
    invoke<SubmitResult>("submit_answer", { lang, exercise, answer, elapsedMs }),
  addCard: (lang: string, entryId: string) =>
    invoke<CardView>("add_card", { lang, entryId }),
  stats: () => invoke<Stats>("stats"),
  dueCards: (lang: string | null, limit: number) =>
    invoke<number>("due_cards", { lang, limit }),
  listDialogs: (lang: string) => invoke<DialogSummary[]>("list_dialogs", { lang }),
  getDialog: (lang: string, id: string) => invoke<Dialog>("get_dialog", { lang, id }),
  lookup: (lang: string, entryId: string) =>
    invoke<Token[]>("lookup", { lang, entryId }),
  define: (lang: string, word: string) =>
    invoke<Entry | null>("define", { lang, word }),
  assetData: (lang: string, rel: string) =>
    invoke<string | null>("asset_data", { lang, rel }),
  getLlmSettings: () => invoke<LlmSettings>("get_llm_settings"),
  setLlmSettings: (baseUrl: string, model: string, apiKey?: string) =>
    invoke<void>("set_llm_settings", { baseUrl, model, apiKey }),
  chatSend: (lang: string, session: string, theme: string, level: number, text: string) =>
    invoke<TutorTurn>("chat_send", { lang, session, theme, level, text }),
  chatHistory: (session: string, limit: number) =>
    invoke<[string, string][]>("chat_history", { session, limit }),
  ttsVoices: (lang: string) => invoke<VoiceInfo[]>("tts_voices", { lang }),
  getVoice: (lang: string) => invoke<string | null>("get_voice", { lang }),
  setVoice: (lang: string, voice: string) => invoke<void>("set_voice", { lang, voice }),
  speak: (lang: string, text: string, entryId?: string) =>
    invoke<string>("speak", { lang, text, entryId }),
};