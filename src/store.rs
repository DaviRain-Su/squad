use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub role: String,
    pub joined_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageRecord {
    pub id: i64,
    pub from_agent: String,
    pub to_agent: String,
    pub content: String,
    pub created_at: i64,
    pub read: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database: {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                role TEXT NOT NULL,
                joined_at INTEGER NOT NULL,
                session_token TEXT
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                read INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        // Migration: add session_token column if missing (existing DBs)
        let _ = conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN session_token TEXT;"
        );
        Ok(Self { conn })
    }

    pub fn register_agent(&self, id: &str, role: &str) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let token = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO agents (id, role, joined_at, session_token) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, role, now, token],
        )?;
        Ok(token)
    }

    pub fn get_session_token(&self, id: &str) -> Result<Option<String>> {
        let token: Option<String> = self.conn.query_row(
            "SELECT session_token FROM agents WHERE id = ?1",
            [id],
            |row| row.get(0),
        ).optional()?;
        Ok(token)
    }

    pub fn unregister_agent(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM agents WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, role, joined_at FROM agents ORDER BY joined_at")?;
        let agents = stmt
            .query_map([], |row| {
                Ok(AgentRecord {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    joined_at: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    pub fn agent_exists(&self, id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM agents WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn agent_names(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM agents ORDER BY id")?;
        let names = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(names)
    }

    pub fn send_message(&self, from: &str, to: &str, content: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO messages (from_agent, to_agent, content, created_at, read) VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params![from, to, content, now],
        )?;
        Ok(())
    }

    pub fn send_message_checked(&self, from: &str, to: &str, content: &str) -> Result<()> {
        if !self.agent_exists(to)? {
            let names = self.agent_names()?;
            anyhow::bail!(
                "{to} does not exist. Online agents: {}",
                names.join(", ")
            );
        }
        self.send_message(from, to, content)
    }

    /// Broadcast a message to all agents except the sender.
    pub fn broadcast_message(&self, from: &str, content: &str) -> Result<Vec<String>> {
        let agents = self.agent_names()?;
        let recipients: Vec<_> = agents.into_iter().filter(|a| a != from).collect();
        for to in &recipients {
            self.send_message(from, to, content)?;
        }
        Ok(recipients)
    }

    /// Atomically read and mark messages as read using a transaction.
    pub fn receive_messages(&self, agent_id: &str) -> Result<Vec<MessageRecord>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "SELECT id, from_agent, to_agent, content, created_at, read
             FROM messages WHERE to_agent = ?1 AND read = 0 ORDER BY created_at",
        )?;
        let messages: Vec<MessageRecord> = stmt
            .query_map([agent_id], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_agent: row.get(1)?,
                    to_agent: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                    read: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        if !messages.is_empty() {
            let max_id = messages.last().unwrap().id;
            tx.execute(
                "UPDATE messages SET read = 1 WHERE to_agent = ?1 AND read = 0 AND id <= ?2",
                rusqlite::params![agent_id, max_id],
            )?;
        }
        tx.commit()?;
        Ok(messages)
    }

    /// Check if there are unread messages for an agent (used by --wait).
    pub fn has_unread_messages(&self, agent_id: &str) -> Result<bool> {
        let has: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM messages WHERE to_agent = ?1 AND read = 0",
            [agent_id],
            |row| row.get(0),
        )?;
        Ok(has)
    }

    pub fn pending_messages(&self) -> Result<Vec<MessageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_agent, to_agent, content, created_at, read
             FROM messages WHERE read = 0 ORDER BY created_at",
        )?;
        let messages = stmt
            .query_map([], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_agent: row.get(1)?,
                    to_agent: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                    read: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    /// All messages (including read), optionally filtered by agent.
    pub fn all_messages(&self, agent_id: Option<&str>) -> Result<Vec<MessageRecord>> {
        fn map_row(row: &rusqlite::Row) -> rusqlite::Result<MessageRecord> {
            Ok(MessageRecord {
                id: row.get(0)?,
                from_agent: row.get(1)?,
                to_agent: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                read: row.get(5)?,
            })
        }

        let messages = match agent_id {
            Some(id) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, from_agent, to_agent, content, created_at, read
                     FROM messages WHERE from_agent = ?1 OR to_agent = ?1 ORDER BY created_at",
                )?;
                let rows = stmt.query_map([id], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, from_agent, to_agent, content, created_at, read
                     FROM messages ORDER BY created_at",
                )?;
                let rows = stmt.query_map([], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
        };
        Ok(messages)
    }
}
