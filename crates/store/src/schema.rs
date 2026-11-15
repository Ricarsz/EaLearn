//! 库结构与迁移。`VERSION` 写进 `PRAGMA user_version`。
//!
//! 迁移策略：版本号不匹配时按 `MIGRATIONS` 逐级执行，不删用户数据。
//! 目前只有 v1（初始结构），后续新增迁移追加到数组尾部即可。

pub const VERSION: i64 = 1;

pub const INIT: &str = r#"
CREATE TABLE IF NOT EXISTS packs (
    lang         TEXT PRIMARY KEY,
    display      TEXT NOT NULL,
    version      TEXT NOT NULL,
    content_hash TEXT,
    enabled      INTEGER NOT NULL DEFAULT 1,
    installed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cards (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    pack       TEXT NOT NULL REFERENCES packs(lang) ON DELETE CASCADE,
    entry_id   TEXT NOT NULL,
    kind       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (pack, entry_id, kind)
);

CREATE TABLE IF NOT EXISTS reviews (
    card_id     INTEGER PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
    stability   REAL NOT NULL,
    difficulty  REAL NOT NULL,
    reps        INTEGER NOT NULL,
    lapses      INTEGER NOT NULL,
    last_review INTEGER,
    due         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reviews_due ON reviews(due);

CREATE TABLE IF NOT EXISTS attempts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id       INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    exercise_kind TEXT NOT NULL,
    correct       INTEGER NOT NULL,
    ratio         REAL NOT NULL,
    elapsed_ms    INTEGER NOT NULL,
    at            INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attempts_at ON attempts(at);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    role       TEXT NOT NULL,
    text       TEXT NOT NULL,
    at         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_session ON chat_log(session_id);
"#;
