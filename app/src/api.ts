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
  wordFromAudio: number;
  recall: number;
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
  | { kind: "word-from-audio"; entry: string; audio: string; options: string[]; answer: number }
  | {
      kind: "recall";
      entry: string;
      surface: string;
      gloss?: string;
      reading?: string;
    }
  | {
      kind: "article-mcq";
      article: string;
      question: number;
      prompt: string;
      options: string[];
      answer: number;
    }
  | { kind: "chat-scenario"; dialog: string };

export type SessionPlan = { exercises: Exercise[]; review: number };

export type Answer =
  | { kind: "order"; order: number[] }
  | { kind: "choice"; index: number }
  | { kind: "recall"; rating: number };

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

export type Question = {
  prompt: string;
  options: string[];
  answer: number;
  explain?: string;
};

export type Article = {
  id: string;
  title: string;
  body: string;
  tokens?: Token[];
  questions?: Question[];
  tags?: string[];
  difficulty?: number;
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

export type ProviderPreset = {
  id: string;
  name: string;
  base_url: string;
  note: string;
};

export type LlmSettings = {
  base_url: string;
  model: string;
  has_key: boolean;
  key_hint: string;
};

export type CardItem = {
  card_id: number;
  entry_id: string;
  surface: string;
  gloss?: string | null;
  kind: string;
  stability: number;
  difficulty: number;
  reps: number;
  lapses: number;
  due: number;
  last_review?: number | null;
  mastery: number;
  is_custom: boolean;
  book: string;
};

export type BookInfo = {
  id: string;
  lang: string;
  name: string;
  count: number;
  imported: number;
};

export type RemoteBook = {
  id: string;
  lang: string;
  /** words / sentences / article；老索引没有这个字段，按 words 处理 */
  kind?: string;
  name: string;
  url: string;
  /** 备用地址，主地址不可达时依次尝试 */
  mirrors?: string[];
  count: number;
  license: string;
  note: string;
  /** 本地已经导入多少条；>0 表示已下载 */
  imported: number;
};

export type ImportResult = { added: number; updated: number; skipped: string[] };

export type DayActivity = {
  date: string;
  attempts: number;
  reviews: number;
  focus_minutes: number;
};

export type DayAccuracy = { date: string; attempts: number; correct: number };
export type KindAccuracy = { kind: string; attempts: number; correct: number; avg_ms: number };
export type DueBucket = { date: string; due: number };
export type WeakEntry = {
  entry_id: string;
  surface: string;
  gloss: string | null;
  total: number;
  wrong: number;
  last_at: number;
};

export type VoiceInfo = { id: string; label: string; engine: string; ready: boolean };

export type VoiceOption = {
  id: string;
  name: string;
  quality: string;
  size_mb: number;
  installed: boolean;
};

export type DialogSummary = { id: string; title: string; theme: string; nodes: number };

export const api = {
  listPacks: () => invoke<PackSummary[]>("list_packs"),
  startSession: (lang: string, limit: number, seed: number, mode: string) =>
    invoke<SessionPlan>("start_session", { lang, limit, seed, mode }),
  submit: (lang: string, exercise: Exercise, answer: Answer, elapsedMs: number) =>
    invoke<SubmitResult>("submit_answer", { lang, exercise, answer, elapsedMs }),
  addCard: (lang: string, entryId: string) =>
    invoke<CardView>("add_card", { lang, entryId }),
  stats: () => invoke<Stats>("stats"),
  dueCards: (lang: string | null, limit: number) =>
    invoke<number>("due_cards", { lang, limit }),
  listDialogs: (lang: string) => invoke<DialogSummary[]>("list_dialogs", { lang }),
  getDialog: (lang: string, id: string) => invoke<Dialog>("get_dialog", { lang, id }),
  getArticle: (lang: string, id: string) => invoke<Article>("get_article", { lang, id }),
  lookup: (lang: string, entryId: string) =>
    invoke<Token[]>("lookup", { lang, entryId }),
  define: (lang: string, word: string) =>
    invoke<Entry | null>("define", { lang, word }),
  llmProviders: () => invoke<ProviderPreset[]>("llm_providers"),
  llmModels: (baseUrl: string, apiKey?: string) =>
    invoke<string[]>("llm_models", { baseUrl, apiKey }),
  getLlmSettings: () => invoke<LlmSettings>("get_llm_settings"),
  setLlmSettings: (baseUrl: string, model: string, apiKey?: string) =>
    invoke<void>("set_llm_settings", { baseUrl, model, apiKey }),
  chatSend: (lang: string, session: string, theme: string, level: number, text: string) =>
    invoke<TutorTurn>("chat_send", { lang, session, theme, level, text }),
  chatHistory: (session: string, limit: number) =>
    invoke<[string, string][]>("chat_history", { session, limit }),
  ttsVoices: (lang: string) => invoke<VoiceInfo[]>("tts_voices", { lang }),
  voiceCatalog: (lang: string) => invoke<VoiceOption[]>("voice_catalog", { lang }),
  installVoice: (id: string) => invoke<void>("install_voice", { id }),
  getVoice: (lang: string) => invoke<string | null>("get_voice", { lang }),
  setVoice: (lang: string, voice: string) => invoke<void>("set_voice", { lang, voice }),
  speak: (lang: string, text: string, entryId?: string, voice?: string) =>
    invoke<string>("speak", { lang, text, entryId, voice }),
  listCards: (lang: string) => invoke<CardItem[]>("list_cards", { lang }),
  deleteCard: (cardId: number) => invoke<void>("delete_card", { cardId }),
  deleteCustomEntry: (lang: string, entryId: string) =>
    invoke<void>("delete_custom_entry", { lang, entryId }),
  importWords: (lang: string, text: string, book?: string) =>
    invoke<ImportResult>("import_words", { lang, text, book }),
  importArticle: (lang: string, text: string) => invoke<string>("import_article", { lang, text }),
  listBooks: () => invoke<BookInfo[]>("list_books"),
  importBook: (id: string) => invoke<ImportResult>("import_book", { id }),
  remoteBooks: (indexUrl?: string) =>
    invoke<RemoteBook[]>("remote_books", { indexUrl }),
  booksIndexUrl: () => invoke<string>("books_index_url"),
  setBooksIndexUrl: (url: string) => invoke<void>("set_books_index_url", { url }),
  installRemoteBook: (lang: string, name: string, url: string, kind?: string, mirrors?: string[]) =>
    invoke<ImportResult>("install_remote_book", { lang, name, url, kind, mirrors }),
  recordFocus: (minutes: number, completed: boolean) =>
    invoke<void>("record_focus", { minutes, completed }),
  activity: (days: number, tzOffsetMin: number) =>
    invoke<DayActivity[]>("activity", { days, tzOffsetMin }),
  dailyAccuracy: (days: number, tzOffsetMin: number) =>
    invoke<DayAccuracy[]>("daily_accuracy", { days, tzOffsetMin }),
  kindAccuracy: (days: number, lang?: string) =>
    invoke<KindAccuracy[]>("kind_accuracy", { days, lang }),
  dueForecast: (days: number, tzOffsetMin: number, lang?: string) =>
    invoke<DueBucket[]>("due_forecast", { days, tzOffsetMin, lang }),
  weakEntries: (lang: string, days: number, limit: number) =>
    invoke<WeakEntry[]>("weak_entries", { lang, days, limit }),
  exportCards: (lang: string) => invoke<string>("export_cards", { lang }),
  backupDb: () => invoke<string>("backup_db"),
  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  resetData: (scope: "progress" | "all") => invoke<void>("reset_data", { scope }),
};