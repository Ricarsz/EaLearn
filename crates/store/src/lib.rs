//! 本地库。全部状态落在一个 SQLite 文件里：装了哪些 pack、有哪些 SRS 卡、
//! 每张卡的调度状态、每次作答、用户设置。
//!
//! 时间一律由调用方传入（Unix 秒），库里不写 `datetime('now')`——这样测试
//! 和「重置到今天」这类操作都能精确控制。

use std::path::Path;

use ealearn_core::exercise::Grade;
use ealearn_core::srs::{CardState, Rating, Scheduler};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub mod schema;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Corner(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "数据库错误: {e}"),
            StoreError::Io(e) => write!(f, "文件错误: {e}"),
            StoreError::Corner(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// 卡片类型。同一份内容可以按不同角度多次入卡（词、句、俚语、语法点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Word,
    Sentence,
    Slang,
    Grammar,
}

impl CardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CardKind::Word => "word",
            CardKind::Sentence => "sentence",
            CardKind::Slang => "slang",
            CardKind::Grammar => "grammar",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "word" => CardKind::Word,
            "sentence" => CardKind::Sentence,
            "slang" => CardKind::Slang,
            "grammar" => CardKind::Grammar,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayActivity {
    /// 本地日期 YYYY-MM-DD
    pub date: String,
    pub attempts: i64,
    pub reviews: i64,
    pub focus_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEntry {
    pub id: String,
    pub surface: String,
    pub gloss: Option<String>,
    pub tags: Vec<String>,
    /// 来自哪本单词书（手工导入为空）
    pub book: String,
}

/// 用户导入的阅读文章。`questions` 是题目数组的 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomArticle {
    pub id: String,
    pub title: String,
    pub body: String,
    pub questions: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardRow {
    pub card_id: i64,
    pub entry_id: String,
    pub kind: CardKind,
    pub state: CardState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackRow {
    pub lang: String,
    pub display: String,
    pub version: String,
    pub content_hash: Option<String>,
    pub enabled: bool,
    pub installed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DueCard {
    pub card_id: i64,
    /// pack 的语言标识（en / ko）
    pub pack: String,
    pub entry_id: String,
    pub kind: CardKind,
    pub state: CardState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub cards: i64,
    pub due_now: i64,
    pub reviews_today: i64,
    pub attempts_today: i64,
    pub accuracy_today: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayAccuracy {
    pub date: String,
    pub attempts: i64,
    pub correct: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KindAccuracy {
    pub kind: String,
    pub attempts: i64,
    pub correct: i64,
    pub avg_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DueBucket {
    pub date: String,
    pub due: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeakEntry {
    pub entry_id: String,
    pub total: i64,
    pub wrong: i64,
    pub last_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportRow {
    pub pack: String,
    pub entry_id: String,
    pub kind: String,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: i64,
    pub lapses: i64,
    pub last_review: i64,
    pub due: i64,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        Self::prepare(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn open_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::prepare(&mut conn)?;
        Ok(Self { conn })
    }

    fn prepare(conn: &mut Connection) -> Result<()> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(schema::INIT)?;

        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 2 {
            // v1 的 custom_entries 没有 book 列；新库由 INIT 建好，这里只补老库
            let _ = conn.execute(
                "ALTER TABLE custom_entries ADD COLUMN book TEXT NOT NULL DEFAULT ''",
                [],
            );
        }
        conn.pragma_update(None, "user_version", schema::VERSION)?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    // ── pack ────────────────────────────────────────────────────────────

    /// 登记或更新一个已安装的 pack。`lang` 既是主键也是 profile 名。
    pub fn upsert_pack(
        &self,
        lang: &str,
        display: &str,
        version: &str,
        content_hash: Option<&str>,
        enabled: bool,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO packs (lang, display, version, content_hash, enabled, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(lang) DO UPDATE SET
                display = excluded.display,
                version = excluded.version,
                content_hash = excluded.content_hash,
                enabled = excluded.enabled",
            params![
                lang,
                display,
                version,
                content_hash,
                i64::from(enabled),
                now
            ],
        )?;
        Ok(())
    }

    pub fn packs(&self) -> Result<Vec<PackRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT lang, display, version, content_hash, enabled, installed_at
             FROM packs ORDER BY display",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PackRow {
                    lang: r.get(0)?,
                    display: r.get(1)?,
                    version: r.get(2)?,
                    content_hash: r.get(3)?,
                    enabled: r.get::<_, i64>(4)? != 0,
                    installed_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn set_enabled(&self, lang: &str, enabled: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE packs SET enabled = ?2 WHERE lang = ?1",
            params![lang, i64::from(enabled)],
        )?;
        if n == 0 {
            return Err(StoreError::Corner(format!("未安装的 pack: {lang}")));
        }
        Ok(())
    }

    // ── 卡片 ────────────────────────────────────────────────────────────

    /// 入卡。已存在则返回原 card_id，不重置调度状态。
    pub fn add_card(&self, pack: &str, entry_id: &str, kind: CardKind, now: i64) -> Result<i64> {
        if let Some(id) = self.card_id(pack, entry_id, kind)? {
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO cards (pack, entry_id, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![pack, entry_id, kind.as_str(), now],
        )?;
        let card_id = self.conn.last_insert_rowid();
        // 新卡立刻到期：用户刚点进来就能练。
        self.conn.execute(
            "INSERT INTO reviews (card_id, stability, difficulty, reps, lapses, last_review, due)
             VALUES (?1, 0, 0, 0, 0, NULL, ?2)",
            params![card_id, now],
        )?;
        Ok(card_id)
    }

    pub fn card_id(&self, pack: &str, entry_id: &str, kind: CardKind) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM cards WHERE pack = ?1 AND entry_id = ?2 AND kind = ?3",
                params![pack, entry_id, kind.as_str()],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn card_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))?)
    }

    /// 已到期的卡，先到期的先出。`packs` 为空表示所有已启用的 pack。
    pub fn due_cards(&self, now: i64, limit: usize, packs: &[&str]) -> Result<Vec<DueCard>> {
        let mut sql = String::from(
            "SELECT c.id, c.pack, c.entry_id, c.kind,
                    r.stability, r.difficulty, r.reps, r.lapses, r.last_review, r.due
             FROM cards c
             JOIN reviews r ON r.card_id = c.id
             JOIN packs p ON p.lang = c.pack
             WHERE r.due <= ?1 AND p.enabled = 1",
        );
        if !packs.is_empty() {
            let placeholders = vec!["?"; packs.len()].join(", ");
            sql.push_str(&format!(" AND c.pack IN ({placeholders})"));
        }
        sql.push_str(" ORDER BY r.due ASC, c.id ASC LIMIT ");
        sql.push_str(&(limit.max(1) as i64).to_string());

        let mut stmt = self.conn.prepare(&sql)?;
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&now];
        for p in packs {
            args.push(p);
        }
        let rows = stmt
            .query_map(args.as_slice(), |r| {
                Ok(DueCard {
                    card_id: r.get(0)?,
                    pack: r.get(1)?,
                    entry_id: r.get(2)?,
                    kind: CardKind::parse(&r.get::<_, String>(3)?).unwrap_or(CardKind::Word),
                    state: CardState {
                        stability: r.get(4)?,
                        difficulty: r.get(5)?,
                        reps: r.get::<_, i64>(6)? as u32,
                        lapses: r.get::<_, i64>(7)? as u32,
                        last_review: r.get(8)?,
                        due: r.get(9)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn review_card(
        &self,
        card_id: i64,
        rating: Rating,
        now: i64,
        scheduler: &Scheduler,
    ) -> Result<CardState> {
        let mut state = self.card_state(card_id)?;
        let next = scheduler.review(&state, rating, now);
        self.conn.execute(
            "UPDATE reviews SET stability = ?2, difficulty = ?3, reps = ?4, lapses = ?5,
                    last_review = ?6, due = ?7
             WHERE card_id = ?1",
            params![
                card_id,
                next.stability,
                next.difficulty,
                next.reps as i64,
                next.lapses as i64,
                next.last_review,
                next.due
            ],
        )?;
        state = next;
        Ok(state)
    }

    pub fn card_state(&self, card_id: i64) -> Result<CardState> {
        self.conn
            .query_row(
                "SELECT stability, difficulty, reps, lapses, last_review, due
                 FROM reviews WHERE card_id = ?1",
                params![card_id],
                |r| {
                    Ok(CardState {
                        stability: r.get(0)?,
                        difficulty: r.get(1)?,
                        reps: r.get::<_, i64>(2)? as u32,
                        lapses: r.get::<_, i64>(3)? as u32,
                        last_review: r.get(4)?,
                        due: r.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::Corner(format!("卡 {card_id} 不存在")))
    }

    /// 记录一次作答。`card_id` 为空表示这次作答没有进复习队列（答对且未入卡）。
    #[allow(clippy::too_many_arguments)]
    pub fn record_attempt(
        &self,
        pack: &str,
        entry_id: &str,
        card_id: Option<i64>,
        exercise_kind: &str,
        grade: &Grade,
        elapsed_ms: i64,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO attempts (pack, entry_id, card_id, exercise_kind, correct, ratio, elapsed_ms, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                pack,
                entry_id,
                card_id,
                exercise_kind,
                i64::from(grade.correct),
                grade.ratio as f64,
                elapsed_ms,
                now
            ],
        )?;
        Ok(())
    }

    pub fn stats(&self, now: i64) -> Result<Stats> {
        let day_start = now - now.rem_euclid(86_400);
        let cards = self
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))?;
        let due_now = self.conn.query_row(
            "SELECT COUNT(*) FROM reviews r JOIN packs p ON p.lang = (
                SELECT c.pack FROM cards c WHERE c.id = r.card_id
             ) WHERE r.due <= ?1 AND p.enabled = 1",
            params![now],
            |r| r.get(0),
        )?;
        let reviews_today = self.conn.query_row(
            "SELECT COUNT(*) FROM reviews WHERE last_review >= ?1",
            params![day_start],
            |r| r.get(0),
        )?;
        let attempts_today: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM attempts WHERE at >= ?1",
            params![day_start],
            |r| r.get(0),
        )?;
        let accuracy = self.conn.query_row(
            "SELECT COALESCE(AVG(correct), 0.0) FROM attempts WHERE at >= ?1",
            params![day_start],
            |r| r.get::<_, f64>(0),
        )?;
        Ok(Stats {
            cards,
            due_now,
            reviews_today,
            attempts_today,
            accuracy_today: accuracy,
        })
    }

    // ── 自定义词条 ──────────────────────────────────────────────────────

    /// 用户导入的词条。id 由自增列派生，`surface` 在同一 pack 内唯一。
    pub fn add_custom_entry(
        &self,
        pack: &str,
        surface: &str,
        gloss: Option<&str>,
        tags: &str,
        book: &str,
        now: i64,
    ) -> Result<String> {
        self.conn.execute(
            "INSERT INTO custom_entries (pack, surface, gloss, tags, book, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(pack, surface) DO UPDATE SET
                gloss = excluded.gloss, tags = excluded.tags,
                book = CASE WHEN excluded.book = '' THEN custom_entries.book ELSE excluded.book END",
            params![pack, surface, gloss, tags, book, now],
        )?;
        let seq: i64 = self.conn.query_row(
            "SELECT seq FROM custom_entries WHERE pack = ?1 AND surface = ?2",
            params![pack, surface],
            |r| r.get(0),
        )?;
        Ok(format!("custom.{pack}.{seq}"))
    }

    pub fn custom_entries(&self, pack: &str) -> Result<Vec<CustomEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, surface, gloss, tags, book FROM custom_entries WHERE pack = ?1 ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![pack], |r| {
                Ok(CustomEntry {
                    id: format!("custom.{pack}.{}", r.get::<_, i64>(0)?),
                    surface: r.get(1)?,
                    gloss: r.get(2)?,
                    tags: r
                        .get::<_, String>(3)?
                        .split(',')
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .map(str::to_string)
                        .collect(),
                    book: r.get::<_, String>(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 批量导入词条。几千个词逐条 transaction 会慢到不可用，这里一次事务做完。
    /// 返回 (id, 是否新建)。
    pub fn bulk_add_custom_entries(
        &self,
        pack: &str,
        rows: &[(String, Option<String>, String, String)],
        now: i64,
    ) -> Result<Vec<(String, bool)>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut out = Vec::with_capacity(rows.len());

        for (surface, gloss, tags, book) in rows {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT seq FROM custom_entries WHERE pack = ?1 AND surface = ?2",
                    params![pack, surface],
                    |r| r.get(0),
                )
                .optional()?;

            let (seq, created) = match existing {
                Some(seq) => {
                    tx.execute(
                        "UPDATE custom_entries SET gloss = ?2, tags = ?3,
                            book = CASE WHEN ?4 = '' THEN book ELSE ?4 END
                         WHERE seq = ?1",
                        params![seq, gloss, tags, book],
                    )?;
                    (seq, false)
                }
                None => {
                    tx.execute(
                        "INSERT INTO custom_entries (pack, surface, gloss, tags, book, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![pack, surface, gloss, tags, book, now],
                    )?;
                    (tx.last_insert_rowid(), true)
                }
            };
            out.push((format!("custom.{pack}.{seq}"), created));
        }

        tx.commit()?;
        Ok(out)
    }

    /// 批量建卡。已存在的卡不重置调度状态。
    pub fn bulk_add_cards(
        &self,
        pack: &str,
        entry_ids: &[String],
        kind: CardKind,
        now: i64,
    ) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut created = 0usize;
        for id in entry_ids {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO cards (pack, entry_id, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![pack, id, kind.as_str(), now],
            )?;
            if changed == 1 {
                let card_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO reviews (card_id, stability, difficulty, reps, lapses, last_review, due)
                     VALUES (?1, 0, 0, 0, 0, NULL, ?2)",
                    params![card_id, now],
                )?;
                created += 1;
            }
        }
        tx.commit()?;
        Ok(created)
    }

    pub fn delete_custom_entry(&self, id: &str) -> Result<()> {
        // id 形如 custom.<pack>.<seq>
        let seq = id.rsplit('.').next().unwrap_or_default();
        self.conn.execute(
            "DELETE FROM custom_entries WHERE seq = ?1",
            params![seq.parse::<i64>().unwrap_or(-1)],
        )?;
        Ok(())
    }

    // ── 自定义文章 ──────────────────────────────────────────────────────

    /// 用户导入的阅读文章。题目以 JSON 存在一列，读出来再反序列化。
    pub fn add_custom_article(
        &self,
        pack: &str,
        title: &str,
        body: &str,
        questions: &str,
        now: i64,
    ) -> Result<String> {
        self.conn.execute(
            "INSERT INTO custom_articles (pack, title, body, questions, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![pack, title, body, questions, now],
        )?;
        Ok(format!("custom.article.{}", self.conn.last_insert_rowid()))
    }

    pub fn custom_articles(&self, pack: &str) -> Result<Vec<CustomArticle>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, title, body, questions FROM custom_articles WHERE pack = ?1 ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![pack], |r| {
                Ok(CustomArticle {
                    id: format!("custom.article.{}", r.get::<_, i64>(0)?),
                    title: r.get(1)?,
                    body: r.get(2)?,
                    questions: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_custom_article(&self, id: &str) -> Result<()> {
        let seq = id.rsplit('.').next().unwrap_or_default();
        self.conn.execute(
            "DELETE FROM custom_articles WHERE seq = ?1",
            params![seq.parse::<i64>().unwrap_or(-1)],
        )?;
        Ok(())
    }

    // ── 清理数据 ────────────────────────────────────────────────────────

    /// 清学习记录：卡片（连带 reviews）、作答、番茄钟、聊天。
    /// 导入的词条/文章与设置都保留。
    pub fn clear_progress(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM attempts;
             DELETE FROM cards;
             DELETE FROM focus_sessions;
             DELETE FROM chat_log;",
        )?;
        Ok(())
    }

    /// 连导入内容与设置一起清空，回到刚装好的状态（语言包由启动时重新登记）。
    pub fn clear_all(&self) -> Result<()> {
        self.clear_progress()?;
        self.conn.execute_batch(
            "DELETE FROM custom_articles;
             DELETE FROM custom_entries;
             DELETE FROM settings;
             DELETE FROM packs;",
        )?;
        Ok(())
    }

    // ── 卡片浏览 ────────────────────────────────────────────────────────

    pub fn cards(&self, pack: &str) -> Result<Vec<CardRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.entry_id, c.kind, r.stability, r.difficulty, r.reps, r.lapses,
                    r.last_review, r.due
             FROM cards c JOIN reviews r ON r.card_id = c.id
             WHERE c.pack = ?1
             ORDER BY r.due ASC, c.id ASC",
        )?;
        let rows = stmt
            .query_map(params![pack], |r| {
                Ok(CardRow {
                    card_id: r.get(0)?,
                    entry_id: r.get(1)?,
                    kind: CardKind::parse(&r.get::<_, String>(2)?).unwrap_or(CardKind::Word),
                    state: CardState {
                        stability: r.get(3)?,
                        difficulty: r.get(4)?,
                        reps: r.get::<_, i64>(5)? as u32,
                        lapses: r.get::<_, i64>(6)? as u32,
                        last_review: r.get(7)?,
                        due: r.get(8)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_card(&self, card_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE id = ?1", params![card_id])?;
        Ok(())
    }

    // ── 专注与打卡 ──────────────────────────────────────────────────────

    pub fn add_focus(&self, minutes: u32, completed: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO focus_sessions (minutes, completed, at) VALUES (?1, ?2, ?3)",
            params![minutes as i64, i64::from(completed), now],
        )?;
        Ok(())
    }

    /// 最近 `days` 天的活动汇总。`tz_offset_min` 是本地时区相对 UTC 的分钟数，
    /// 打卡必须按本地日界切，不能按 UTC。
    pub fn activity(&self, days: u32, now: i64, tz_offset_min: i32) -> Result<Vec<DayActivity>> {
        let shift = tz_offset_min as i64 * 60;
        let today = (now + shift).div_euclid(86_400);
        let from_day = today - days as i64 + 1;

        let mut buckets: std::collections::BTreeMap<i64, DayActivity> =
            std::collections::BTreeMap::new();
        for d in from_day..=today {
            buckets.insert(
                d,
                DayActivity {
                    date: ymd_from_days(d),
                    attempts: 0,
                    reviews: 0,
                    focus_minutes: 0,
                },
            );
        }

        let mut stmt = self
            .conn
            .prepare("SELECT correct, at FROM attempts WHERE at >= ?1")?;
        let attempts = stmt
            .query_map(params![from_day * 86_400 - shift], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (_correct, at) in attempts {
            if let Some(d) = buckets.get_mut(&((at + shift).div_euclid(86_400))) {
                d.attempts += 1;
            }
        }

        let mut stmt = self
            .conn
            .prepare("SELECT last_review FROM reviews WHERE last_review >= ?1")?;
        let reviews = stmt
            .query_map(params![from_day * 86_400 - shift], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for at in reviews {
            if let Some(d) = buckets.get_mut(&((at + shift).div_euclid(86_400))) {
                d.reviews += 1;
            }
        }

        let mut stmt = self
            .conn
            .prepare("SELECT minutes, at FROM focus_sessions WHERE at >= ?1")?;
        let focuses = stmt
            .query_map(params![from_day * 86_400 - shift], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (minutes, at) in focuses {
            if let Some(d) = buckets.get_mut(&((at + shift).div_euclid(86_400))) {
                d.focus_minutes += minutes;
            }
        }

        Ok(buckets.into_values().collect())
    }

    // ── 统计与导出 ──────────────────────────────────────────────────────

    /// 每日作答数与正确数，空白日补 0。
    pub fn daily_accuracy(
        &self,
        days: u32,
        now: i64,
        tz_offset_min: i32,
    ) -> Result<Vec<DayAccuracy>> {
        let shift = tz_offset_min as i64 * 60;
        let today = (now + shift).div_euclid(86_400);
        let from_day = today - days as i64 + 1;

        let mut buckets: std::collections::BTreeMap<i64, DayAccuracy> =
            std::collections::BTreeMap::new();
        for d in from_day..=today {
            buckets.insert(
                d,
                DayAccuracy {
                    date: ymd_from_days(d),
                    attempts: 0,
                    correct: 0,
                },
            );
        }

        let mut stmt = self
            .conn
            .prepare("SELECT correct, at FROM attempts WHERE at >= ?1")?;
        let rows = stmt
            .query_map(params![from_day * 86_400 - shift], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (correct, at) in rows {
            if let Some(d) = buckets.get_mut(&((at + shift).div_euclid(86_400))) {
                d.attempts += 1;
                if correct != 0 {
                    d.correct += 1;
                }
            }
        }
        Ok(buckets.into_values().collect())
    }

    /// 各题型的作答量、正确数与平均耗时。
    pub fn kind_accuracy(&self, since: i64, packs: &[&str]) -> Result<Vec<KindAccuracy>> {
        let mut stmt = self.conn.prepare(
            "SELECT pack, exercise_kind, correct, elapsed_ms FROM attempts WHERE at >= ?1",
        )?;
        let rows = stmt
            .query_map(params![since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut map: std::collections::BTreeMap<String, (i64, i64, i64)> =
            std::collections::BTreeMap::new();
        for (pack, kind, correct, ms) in rows {
            if !packs.is_empty() && !packs.contains(&pack.as_str()) {
                continue;
            }
            let e = map.entry(kind).or_insert((0, 0, 0));
            e.0 += 1;
            if correct != 0 {
                e.1 += 1;
            }
            e.2 += ms;
        }
        Ok(map
            .into_iter()
            .map(|(kind, (attempts, correct, ms))| KindAccuracy {
                kind,
                attempts,
                correct,
                avg_ms: if attempts > 0 {
                    ms as f64 / attempts as f64
                } else {
                    0.0
                },
            })
            .collect())
    }

    /// 未来若干天的到期卡数（含今天）。
    pub fn due_forecast(
        &self,
        days: u32,
        now: i64,
        tz_offset_min: i32,
        packs: &[&str],
    ) -> Result<Vec<DueBucket>> {
        let shift = tz_offset_min as i64 * 60;
        let today = (now + shift).div_euclid(86_400);

        let mut buckets: std::collections::BTreeMap<i64, DueBucket> =
            std::collections::BTreeMap::new();
        for d in today..=today + days as i64 {
            buckets.insert(
                d,
                DueBucket {
                    date: ymd_from_days(d),
                    due: 0,
                },
            );
        }

        let mut stmt = self
            .conn
            .prepare("SELECT r.due, c.pack FROM reviews r JOIN cards c ON c.id = r.card_id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (due, pack) in rows {
            if !packs.is_empty() && !packs.contains(&pack.as_str()) {
                continue;
            }
            let day = (due + shift).div_euclid(86_400);
            if let Some(b) = buckets.get_mut(&day) {
                b.due += 1;
            }
        }
        Ok(buckets.into_values().collect())
    }

    /// 一段时间内错得最多的词条。
    pub fn weak_entries(&self, since: i64, packs: &[&str], limit: usize) -> Result<Vec<WeakEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pack, entry_id, correct, at FROM attempts WHERE at >= ?1")?;
        let rows = stmt
            .query_map(params![since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut map: std::collections::HashMap<(String, String), (i64, i64, i64)> =
            std::collections::HashMap::new();
        for (pack, entry_id, correct, at) in rows {
            if !packs.is_empty() && !packs.contains(&pack.as_str()) {
                continue;
            }
            let e = map.entry((pack, entry_id)).or_insert((0, 0, 0));
            e.0 += 1;
            if correct == 0 {
                e.1 += 1;
            }
            e.2 = e.2.max(at);
        }

        let mut out: Vec<WeakEntry> = map
            .into_iter()
            .filter(|(_, (_, wrong, _))| *wrong > 0)
            .map(|((_, entry_id), (total, wrong, last_at))| WeakEntry {
                entry_id,
                total,
                wrong,
                last_at,
            })
            .collect();
        out.sort_by(|a, b| b.wrong.cmp(&a.wrong).then(b.last_at.cmp(&a.last_at)));
        out.truncate(limit);
        Ok(out)
    }

    /// 导出卡片复习状态（词面与释义由上层用词条表补）。
    pub fn export_rows(&self, packs: &[&str]) -> Result<Vec<ExportRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.pack, c.entry_id, c.kind, r.stability, r.difficulty, r.reps, r.lapses,
                    COALESCE(r.last_review, 0), r.due
             FROM cards c JOIN reviews r ON r.card_id = c.id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ExportRow {
                    pack: r.get(0)?,
                    entry_id: r.get(1)?,
                    kind: r.get(2)?,
                    stability: r.get(3)?,
                    difficulty: r.get(4)?,
                    reps: r.get(5)?,
                    lapses: r.get(6)?,
                    last_review: r.get(7)?,
                    due: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter(|r| packs.is_empty() || packs.contains(&r.pack.as_str()))
            .collect())
    }

    /// 把整库备份成一个独立的 SQLite 文件（VACUUM INTO，写入过程中也是一致的）。
    pub fn backup_to(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.conn.execute(
            "VACUUM INTO ?1",
            params![path.to_string_lossy().into_owned()],
        )?;
        Ok(())
    }

    // ── 设置 ────────────────────────────────────────────────────────────

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    // ── 聊天记录（自由聊天模式） ───────────────────────────────────────

    pub fn append_chat(&self, session: &str, role: &str, text: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO chat_log (session_id, role, text, at) VALUES (?1, ?2, ?3, ?4)",
            params![session, role, text, now],
        )?;
        Ok(())
    }

    pub fn chat_history(&self, session: &str, limit: usize) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, text FROM chat_log WHERE session_id = ?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![session, limit as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().rev().collect())
    }
}

/// 1970-01-01 起的天数 → 公历日期（Howard Hinnant 的 civil_from_days）。
fn ymd_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ealearn_core::exercise::{Answer, Exercise};

    const NOW: i64 = 1_760_000_000;

    fn store() -> Store {
        let s = Store::open_memory().unwrap();
        s.upsert_pack("en", "英语", "0.1.0", Some("abc"), true, NOW)
            .unwrap();
        s
    }

    #[test]
    fn schema_is_at_current_version() {
        let s = Store::open_memory().unwrap();
        assert_eq!(s.schema_version().unwrap(), schema::VERSION);
    }

    #[test]
    fn pack_roundtrip() {
        let s = store();
        let packs = s.packs().unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].lang, "en");
        assert!(packs[0].enabled);

        s.set_enabled("en", false).unwrap();
        assert!(!s.packs().unwrap()[0].enabled);
        assert!(s.set_enabled("ko", false).is_err());
    }

    #[test]
    fn new_card_is_due_immediately_and_not_duplicated() {
        let s = store();
        let a = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        let b = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        assert_eq!(a, b, "同一 (pack, entry, kind) 不应重复建卡");

        let due = s.due_cards(NOW, 10, &[]).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].entry_id, "en.w.0001");
        assert_eq!(due[0].state.reps, 0);
    }

    #[test]
    fn review_pushes_card_out_of_due_queue() {
        let s = store();
        let id = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        let state = s
            .review_card(id, Rating::Good, NOW, &Scheduler::default())
            .unwrap();
        assert_eq!(state.reps, 1);
        assert!(state.due > NOW + 86_400, "Good 之后不该明天就到期");

        assert!(s.due_cards(NOW + 60, 10, &[]).unwrap().is_empty());
        assert_eq!(s.due_cards(state.due, 10, &[]).unwrap().len(), 1);
    }

    #[test]
    fn disabled_pack_is_excluded_from_due_queue() {
        let s = store();
        s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        s.set_enabled("en", false).unwrap();
        assert!(s.due_cards(NOW, 10, &[]).unwrap().is_empty());
    }

    #[test]
    fn pack_filter_narrows_queue() {
        let s = store();
        s.upsert_pack("ko", "韩语", "0.1.0", None, true, NOW)
            .unwrap();
        s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        s.add_card("ko", "ko.w.0001", CardKind::Word, NOW).unwrap();

        assert_eq!(s.due_cards(NOW, 10, &[]).unwrap().len(), 2);
        let only_ko = s.due_cards(NOW, 10, &["ko"]).unwrap();
        assert_eq!(only_ko.len(), 1);
        assert_eq!(only_ko[0].pack, "ko");
    }

    #[test]
    fn attempts_and_stats() {
        let s = store();
        let id = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        let ex = Exercise::WordFromAudio {
            entry: "en.w.0001".into(),
            audio: "media/audio/a.wav".into(),
            options: vec!["gonna".into(), "wanna".into()],
            answer: 0,
        };
        let right = ex.grade(&Answer::Choice { index: 0 }).unwrap();
        let wrong = ex.grade(&Answer::Choice { index: 1 }).unwrap();
        s.record_attempt(
            "en",
            "en.w.0001",
            Some(id),
            "word-from-audio",
            &right,
            1200,
            NOW,
        )
        .unwrap();
        s.record_attempt(
            "en",
            "en.w.0001",
            Some(id),
            "word-from-audio",
            &wrong,
            3400,
            NOW + 10,
        )
        .unwrap();

        let stats = s.stats(NOW + 20).unwrap();
        assert_eq!(stats.cards, 1);
        assert_eq!(stats.attempts_today, 2);
        assert!((stats.accuracy_today - 0.5).abs() < 1e-9);
        assert_eq!(stats.due_now, 1);
    }

    #[test]
    fn daily_accuracy_and_kind_breakdown() {
        let s = store();
        s.record_attempt("en", "en.w.1", None, "word", &grade_eq(), 1000, NOW)
            .unwrap();
        s.record_attempt("en", "en.w.2", None, "word", &grade_ne(), 2000, NOW + 60)
            .unwrap();
        s.record_attempt(
            "en",
            "en.s.1",
            None,
            "sentence",
            &grade_eq(),
            3000,
            NOW + 86_400,
        )
        .unwrap();

        let days = s.daily_accuracy(3, NOW + 86_400, 0).unwrap();
        assert_eq!(days.len(), 3);
        assert_eq!(days[2].attempts, 1);
        assert_eq!(days[1].attempts, 2);
        assert_eq!(days[1].correct, 1);

        let kinds = s.kind_accuracy(0, &["en"]).unwrap();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.iter().any(|k| k.kind == "word" && k.attempts == 2));
        assert!(s.kind_accuracy(0, &["ko"]).unwrap().is_empty());
    }

    #[test]
    fn due_forecast_and_weak_entries() {
        let s = store();
        let id = s.add_card("en", "en.w.1", CardKind::Word, NOW).unwrap();
        s.review_card(id, Rating::Good, NOW, &Scheduler::default())
            .unwrap();

        let forecast = s.due_forecast(3, NOW, 0, &["en"]).unwrap();
        assert_eq!(forecast.len(), 4);
        assert_eq!(forecast.iter().map(|b| b.due).sum::<i64>(), 1);
        assert!(s
            .due_forecast(3, NOW, 0, &["ko"])
            .unwrap()
            .iter()
            .all(|b| b.due == 0));

        for i in 0..3 {
            let g = if i < 2 { grade_ne() } else { grade_eq() };
            s.record_attempt("en", "en.w.1", Some(id), "word", &g, 900, NOW + i)
                .unwrap();
        }
        s.record_attempt("en", "en.w.2", None, "word", &grade_eq(), 900, NOW)
            .unwrap();

        let weak = s.weak_entries(0, &["en"], 10).unwrap();
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].entry_id, "en.w.1");
        assert_eq!(weak[0].wrong, 2);
        assert_eq!(weak[0].total, 3);

        let rows = s.export_rows(&["en"]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entry_id, "en.w.1");
        assert!(rows[0].stability > 0.0);
        assert!(s.export_rows(&["ko"]).unwrap().is_empty());
    }

    #[test]
    fn backup_writes_a_readable_copy() {
        let s = store();
        s.add_card("en", "en.w.1", CardKind::Word, NOW).unwrap();
        let dir = std::env::temp_dir().join(format!("ealearn-backup-test-{}", std::process::id()));
        let path = dir.join("e.db");
        s.backup_to(&path).unwrap();
        assert!(path.is_file());

        let copy = Store::open(&path).unwrap();
        assert_eq!(copy.cards("en").unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn grade_eq() -> ealearn_core::exercise::Grade {
        let ex = Exercise::WordFromAudio {
            entry: "e".into(),
            audio: "a.wav".into(),
            options: vec!["a".into(), "b".into()],
            answer: 0,
        };
        ex.grade(&Answer::Choice { index: 0 }).unwrap()
    }

    fn grade_ne() -> ealearn_core::exercise::Grade {
        let ex = Exercise::WordFromAudio {
            entry: "e".into(),
            audio: "a.wav".into(),
            options: vec!["a".into(), "b".into()],
            answer: 0,
        };
        ex.grade(&Answer::Choice { index: 1 }).unwrap()
    }

    #[test]
    fn custom_entries_roundtrip_and_dedupe() {
        let s = store();
        let id = s
            .add_custom_entry("en", "ghost", Some("玩消失"), "slang", "", NOW)
            .unwrap();
        assert!(id.starts_with("custom.en."), "id 形状不对: {id}");

        let list = s.custom_entries("en").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].surface, "ghost");
        assert_eq!(list[0].tags, vec!["slang"]);

        // 同一个词再导入一次不应新增，只更新释义
        let again = s
            .add_custom_entry("en", "ghost", Some("失联"), "", "", NOW)
            .unwrap();
        assert_eq!(again, id);
        let list = s.custom_entries("en").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].gloss.as_deref(), Some("失联"));
    }

    #[test]
    fn bulk_import_is_idempotent() {
        let s = store();
        let rows = vec![
            (
                "ghost".to_string(),
                Some("玩消失".to_string()),
                "slang".to_string(),
                "daily".to_string(),
            ),
            ("vibe".to_string(), None, String::new(), "daily".to_string()),
        ];
        let first = s.bulk_add_custom_entries("en", &rows, NOW).unwrap();
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|(_, created)| *created));

        let ids: Vec<String> = first.iter().map(|(id, _)| id.clone()).collect();
        assert_eq!(
            s.bulk_add_cards("en", &ids, CardKind::Word, NOW).unwrap(),
            2
        );
        // 再来一次不新增卡
        assert_eq!(
            s.bulk_add_cards("en", &ids, CardKind::Word, NOW).unwrap(),
            0
        );

        let second = s.bulk_add_custom_entries("en", &rows, NOW).unwrap();
        assert!(second.iter().all(|(_, created)| !created));
        assert_eq!(second[0].0, first[0].0, "重复导入应复用同一条");
        assert_eq!(s.custom_entries("en").unwrap().len(), 2);
        assert_eq!(s.custom_entries("en").unwrap()[0].book, "daily");
    }

    #[test]
    fn clear_progress_keeps_imported_content() {
        let s = store();
        s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        let id = s
            .add_custom_entry("en", "ghost", Some("玩消失"), "", "", NOW)
            .unwrap();
        s.add_focus(25, true, NOW).unwrap();
        s.set_setting("tts_voice:en", "piper:en_US-lessac").unwrap();

        s.clear_progress().unwrap();
        assert_eq!(s.card_count().unwrap(), 0);
        assert_eq!(s.custom_entries("en").unwrap().len(), 1);
        assert_eq!(s.get_setting("tts_voice:en").unwrap().is_some(), true);
        assert!(id.starts_with("custom."));
    }

    #[test]
    fn clear_all_wipes_everything() {
        let s = store();
        s.add_custom_entry("en", "ghost", None, "", "", NOW)
            .unwrap();
        s.add_custom_article("en", "t", "b", "[]", NOW).unwrap();
        s.set_setting("k", "v").unwrap();

        s.clear_all().unwrap();
        assert_eq!(s.custom_entries("en").unwrap().len(), 0);
        assert_eq!(s.custom_articles("en").unwrap().len(), 0);
        assert_eq!(s.get_setting("k").unwrap(), None);
        assert!(s.packs().unwrap().is_empty());
    }

    #[test]
    fn custom_articles_roundtrip() {
        let s = store();
        let id = s
            .add_custom_article(
                "en",
                "标题",
                "正文",
                r#"[{"prompt":"q","options":["a","b"],"answer":1}]"#,
                NOW,
            )
            .unwrap();
        assert!(id.starts_with("custom.article."));
        let list = s.custom_articles("en").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "标题");
        s.delete_custom_article(&id).unwrap();
        assert!(s.custom_articles("en").unwrap().is_empty());
    }

    #[test]
    fn cards_can_be_listed_and_deleted() {
        let s = store();
        let a = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        s.add_card("en", "en.w.0002", CardKind::Word, NOW).unwrap();

        let rows = s.cards("en").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].card_id, a);

        s.delete_card(a).unwrap();
        let rows = s.cards("en").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entry_id, "en.w.0002");
    }

    #[test]
    fn ymd_matches_known_dates() {
        assert_eq!(ymd_from_days(0), "1970-01-01");
        assert_eq!(ymd_from_days(20_000), "2024-10-04");
        assert_eq!(ymd_from_days(19_723), "2024-01-01");
    }

    #[test]
    fn activity_buckets_by_local_day() {
        let s = store();
        let id = s.add_card("en", "en.w.0001", CardKind::Word, NOW).unwrap();
        let ex = Exercise::WordFromAudio {
            entry: "en.w.0001".into(),
            audio: "media/audio/a.wav".into(),
            options: vec!["gonna".into(), "wanna".into()],
            answer: 0,
        };
        let g = ex.grade(&Answer::Choice { index: 0 }).unwrap();
        s.record_attempt("en", "en.w.0001", Some(id), "word-from-audio", &g, 100, NOW)
            .unwrap();
        s.add_focus(25, true, NOW).unwrap();

        let days = s.activity(7, NOW, 0).unwrap();
        assert_eq!(days.len(), 7);
        let today = days.last().unwrap();
        assert_eq!(today.attempts, 1);
        assert_eq!(today.focus_minutes, 25);
        assert_eq!(today.date, ymd_from_days(NOW.div_euclid(86_400)));

        // 七天前的活动不应落在今天的桶里
        s.add_focus(25, true, NOW - 6 * 86_400).unwrap();
        let days = s.activity(7, NOW, 0).unwrap();
        assert_eq!(days.first().unwrap().focus_minutes, 25);
        assert_eq!(days.last().unwrap().focus_minutes, 25);
    }

    #[test]
    fn settings_roundtrip() {
        let s = Store::open_memory().unwrap();
        assert_eq!(s.get_setting("tts_engine").unwrap(), None);
        s.set_setting("tts_engine", "local").unwrap();
        s.set_setting("tts_engine", "cloud").unwrap();
        assert_eq!(
            s.get_setting("tts_engine").unwrap().as_deref(),
            Some("cloud")
        );
    }

    #[test]
    fn chat_history_is_chronological() {
        let s = Store::open_memory().unwrap();
        s.append_chat("s1", "user", "hey", NOW).unwrap();
        s.append_chat("s1", "tutor", "hi", NOW + 1).unwrap();
        let h = s.chat_history("s1", 10).unwrap();
        assert_eq!(
            h,
            vec![
                ("user".to_string(), "hey".to_string()),
                ("tutor".to_string(), "hi".to_string())
            ]
        );
    }
}
