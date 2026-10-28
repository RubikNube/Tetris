use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use std::{path::Path, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub id: String,
    pub finished_at: String,
    pub score: u64,
    pub lines: u32,
    pub start_level: u32,
    pub final_level: u32,
    pub duration_ms: u64,
    pub completed: bool,
}

pub struct History {
    connection: Connection,
}
impl History {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create history directory {}", parent.display()))?;
        }
        let mut connection = Connection::open(path)
            .with_context(|| format!("cannot open history {}", path.display()))?;
        connection.busy_timeout(Duration::from_millis(750))?;
        let tx = connection.transaction()?;
        let version: u32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match version {
            0 => {
                tx.execute_batch(
                    "CREATE TABLE attempts (
                    id TEXT PRIMARY KEY NOT NULL,
                    finished_at TEXT NOT NULL,
                    score INTEGER NOT NULL CHECK(score >= 0),
                    lines INTEGER NOT NULL CHECK(lines >= 0),
                    start_level INTEGER NOT NULL,
                    final_level INTEGER NOT NULL,
                    duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0),
                    completed INTEGER NOT NULL CHECK(completed IN (0, 1))
                );
                CREATE INDEX attempts_ranking ON attempts(completed, score DESC, finished_at ASC);
                CREATE INDEX attempts_recent ON attempts(finished_at DESC);
                PRAGMA user_version = 1;",
                )?;
            }
            1 => {}
            _ => bail!("history schema {version} is newer than supported schema 1"),
        }
        tx.commit()?;
        // Validate an existing database now, rather than waiting for the first score save.
        connection.prepare("SELECT id, finished_at, score, lines, start_level, final_level, duration_ms, completed FROM attempts LIMIT 0")?;
        Ok(Self { connection })
    }
    pub fn save(&mut self, attempt: &Attempt) -> Result<()> {
        let score = i64::try_from(attempt.score).context("score exceeds SQLite integer range")?;
        let duration =
            i64::try_from(attempt.duration_ms).context("duration exceeds SQLite integer range")?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO attempts (id, finished_at, score, lines, start_level, final_level, duration_ms, completed)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO NOTHING",
            params![attempt.id, attempt.finished_at, score, attempt.lines, attempt.start_level, attempt.final_level, duration, attempt.completed])?;
        tx.commit().context("cannot commit score")
    }
    pub fn top(&self) -> Result<Vec<Attempt>> {
        self.query("SELECT id, finished_at, score, lines, start_level, final_level, duration_ms, completed
            FROM attempts WHERE completed = 1 ORDER BY score DESC, finished_at ASC, id ASC LIMIT 10", 0)
    }
    pub fn recent(&self, page: usize) -> Result<Vec<Attempt>> {
        self.query(
            "SELECT id, finished_at, score, lines, start_level, final_level, duration_ms, completed
            FROM attempts ORDER BY finished_at DESC, id DESC LIMIT 10 OFFSET ?1",
            page.saturating_mul(10),
        )
    }
    fn query(&self, sql: &str, offset: usize) -> Result<Vec<Attempt>> {
        let mut stmt = self.connection.prepare(sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(Attempt {
                id: row.get(0)?,
                finished_at: row.get(1)?,
                score: read_u64(row, 2)?,
                lines: row.get(3)?,
                start_level: row.get(4)?,
                final_level: row.get(5)?,
                duration_ms: read_u64(row, 6)?,
                completed: row.get(7)?,
            })
        };
        let rows = if stmt.parameter_count() == 0 {
            stmt.query_map([], map)?
        } else {
            stmt.query_map(
                [i64::try_from(offset).context("history offset is too large")?],
                map,
            )?
        };
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn read_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    value.try_into().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attempt(id: usize, score: u64, completed: bool) -> Attempt {
        Attempt {
            id: id.to_string(),
            finished_at: format!("2026-10-28T20:{id:02}:00.000Z"),
            score,
            lines: 10,
            start_level: 1,
            final_level: 2,
            duration_ms: 1234,
            completed,
        }
    }
    #[test]
    fn history_survives_restart_with_ranking_paging_and_idempotency() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        {
            let mut h = History::open(&path).unwrap();
            for i in 0..25 {
                let a = attempt(i, (i % 3) as u64, i != 24);
                h.save(&a).unwrap();
                h.save(&a).unwrap();
            }
        }
        let h = History::open(&path).unwrap();
        let top = h.top().unwrap();
        assert_eq!(top.len(), 10);
        assert!(top.iter().all(|a| a.completed));
        assert_eq!(top[0].id, "2");
        assert_eq!(top[1].id, "5");
        let recent = h.recent(0).unwrap();
        assert_eq!(recent[0].id, "24");
        assert!(!recent[0].completed);
        assert_eq!(h.recent(1).unwrap().len(), 10);
        assert_eq!(h.recent(2).unwrap().len(), 5);
        assert!(h.recent(3).unwrap().is_empty());
    }
    #[test]
    fn corrupt_and_newer_databases_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.db");
        std::fs::write(&path, b"not sqlite").unwrap();
        assert!(History::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not sqlite");
        let newer = dir.path().join("new.db");
        let c = Connection::open(&newer).unwrap();
        c.pragma_update(None, "user_version", 99).unwrap();
        assert!(History::open(&newer).is_err());
    }
    #[test]
    fn failed_writes_can_be_retried_without_duplicate_scores() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let mut h = History::open(&path).unwrap();
        h.connection
            .pragma_update(None, "query_only", true)
            .unwrap();
        let a = attempt(1, 400, true);
        assert!(h.save(&a).is_err());
        h.connection
            .pragma_update(None, "query_only", false)
            .unwrap();
        h.save(&a).unwrap();
        h.save(&a).unwrap();
        assert_eq!(h.top().unwrap(), vec![a]);
    }
}
