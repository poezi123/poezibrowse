use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    Normal,
    Tor,
}

impl Shelf {
    fn table(self) -> &'static str {
        match self {
            Shelf::Normal => "bookmarks",
            Shelf::Tor => "tor_bookmarks",
        }
    }
}

impl From<crate::config::Mode> for Shelf {
    fn from(mode: crate::config::Mode) -> Shelf {
        if mode == crate::config::Mode::Tor { Shelf::Tor } else { Shelf::Normal }
    }
}

pub struct Entry {
    pub url: String,
    pub title: String,
    pub time: i64,
}

pub struct Storage {
    db: Connection,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn worth_remembering(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

impl Storage {
    pub fn open() -> Storage {
        let dir = config::data_dir();
        let _ = config::private_dir(&dir);
        let path = dir.join("places.sqlite");
        let db = Connection::open(&path)
            .or_else(|e| {
                eprintln!("poezibrowse: history database unavailable ({e}), using memory");
                Connection::open_in_memory()
            })
            .expect("sqlite in-memory database");
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Self::with_connection(db)
    }

    fn with_connection(db: Connection) -> Storage {
        let _ = db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS history (
                 url TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '',
                 visits INTEGER NOT NULL DEFAULT 1, last_visit INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS history_last ON history(last_visit);
             CREATE TABLE IF NOT EXISTS bookmarks (
                 url TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', added INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS tor_bookmarks (
                 url TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', added INTEGER NOT NULL);",
        );
        Storage { db }
    }

    pub fn record_visit(&self, url: &str, title: &str) {
        if !worth_remembering(url) {
            return;
        }
        let _ = self.db.execute(
            "INSERT INTO history (url, title, visits, last_visit) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET visits = visits + 1, last_visit = ?3,
                 title = CASE WHEN ?2 = '' THEN title ELSE ?2 END",
            params![url, title, now()],
        );
    }

    pub fn suggest(&self, text: &str, limit: usize, shelf: Shelf) -> Vec<Entry> {
        let pattern = format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        if shelf == Shelf::Tor {
            return self.query(
                "SELECT url, title, added FROM tor_bookmarks WHERE url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\'
                 ORDER BY added DESC LIMIT ?2",
                params![pattern, limit as i64],
            );
        }
        let sql = "SELECT url, title, last_visit FROM (
                       SELECT url, title, added AS last_visit, 1000000 AS score FROM bookmarks
                       UNION ALL
                       SELECT url, title, last_visit, visits * 10 + last_visit / 86400 AS score FROM history)
                   WHERE url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\'
                   GROUP BY url ORDER BY MAX(score) DESC LIMIT ?2";
        self.query(sql, params![pattern, limit as i64])
    }

    pub fn history(&self, search: &str, limit: usize) -> Vec<Entry> {
        let pattern = format!("%{}%", search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        self.query(
            "SELECT url, title, last_visit FROM history
             WHERE url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\'
             ORDER BY last_visit DESC LIMIT ?2",
            params![pattern, limit as i64],
        )
    }

    pub fn delete_history_entry(&self, url: &str) {
        let _ = self.db.execute("DELETE FROM history WHERE url = ?1", params![url]);
    }

    pub fn clear_history(&self) {
        let _ = self.db.execute("DELETE FROM history", []);
        let _ = self.db.execute_batch("VACUUM");
    }

    pub fn bookmarks(&self, shelf: Shelf) -> Vec<Entry> {
        self.query(&format!("SELECT url, title, added FROM {} ORDER BY added DESC", shelf.table()), [])
    }

    pub fn is_bookmarked(&self, shelf: Shelf, url: &str) -> bool {
        self.db
            .query_row(&format!("SELECT 1 FROM {} WHERE url = ?1", shelf.table()), params![url], |_| Ok(()))
            .is_ok()
    }

    pub fn add_bookmark(&self, shelf: Shelf, url: &str, title: &str) {
        let _ = self.db.execute(
            &format!(
                "INSERT INTO {} (url, title, added) VALUES (?1, ?2, ?3) ON CONFLICT(url) DO UPDATE SET title = ?2",
                shelf.table()
            ),
            params![url, title, now()],
        );
    }

    pub fn rename_bookmark(&self, shelf: Shelf, url: &str, title: &str) {
        let _ = self.db.execute(&format!("UPDATE {} SET title = ?2 WHERE url = ?1", shelf.table()), params![url, title]);
    }

    pub fn remove_bookmark(&self, shelf: Shelf, url: &str) {
        let _ = self.db.execute(&format!("DELETE FROM {} WHERE url = ?1", shelf.table()), params![url]);
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Vec<Entry> {
        let Ok(mut stmt) = self.db.prepare_cached(sql) else { return Vec::new() };
        stmt.query_map(params, |row| {
            Ok(Entry { url: row.get(0)?, title: row.get(1)?, time: row.get(2)? })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> Storage {
        Storage::with_connection(Connection::open_in_memory().unwrap())
    }

    #[test]
    fn history_and_suggestions() {
        let s = storage();
        s.record_visit("https://rust-lang.org/", "Rust");
        s.record_visit("https://rust-lang.org/", "");
        s.record_visit("https://example.com/", "Example");
        s.record_visit("poezi://start", "Neuer Tab");
        assert_eq!(s.history("", 10).len(), 2);
        let hits = s.suggest("rust", 5, Shelf::Normal);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust", "empty titles must not overwrite known ones");
        assert!(s.suggest("100%", 5, Shelf::Normal).is_empty());
        s.clear_history();
        assert!(s.history("", 10).is_empty());
    }

    #[test]
    fn bookmarks() {
        let s = storage();
        s.add_bookmark(Shelf::Normal, "https://a.org/", "A");
        assert!(s.is_bookmarked(Shelf::Normal, "https://a.org/"));
        assert!(!s.is_bookmarked(Shelf::Tor, "https://a.org/"), "Tor favorites are separate");
        s.record_visit("https://a.org/x", "Ax");
        assert_eq!(s.suggest("a.org", 5, Shelf::Normal)[0].url, "https://a.org/", "bookmarks rank first");
        assert!(s.suggest("a.org", 5, Shelf::Tor).is_empty(), "Tor suggestions never show normal history");
        s.add_bookmark(Shelf::Tor, "http://x.onion/", "X");
        assert_eq!(s.bookmarks(Shelf::Tor).len(), 1);
        s.remove_bookmark(Shelf::Normal, "https://a.org/");
        assert!(!s.is_bookmarked(Shelf::Normal, "https://a.org/"));
    }
}
