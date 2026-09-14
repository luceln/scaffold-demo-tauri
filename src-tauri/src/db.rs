//! SQLite 连接与表结构。
//!
//! 这里刻意**只放连接管理与 DDL**，不掺业务。业务在 `demo_item`。

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::AppError;

/// 表结构 —— **单一事实源**。
///
/// 生产启动与测试都执行这一份字符串，因此不存在"测试库和真实库 schema 漂移"
/// 这种事后极难定位的问题。需要演进时改这里，并接受"老库需要手工迁移"的现实
/// （模板不引入迁移框架：桌面应用的 schema 演进复杂度还撑不起那套机器）。
///
/// 全部用 `IF NOT EXISTS`：重复执行安全，`open()` 可以被反复调用。
pub const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS demo_item (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    quantity   INTEGER NOT NULL DEFAULT 0,
    note       TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- 按名字查（将来做搜索时用得上）。显式建索引而不是指望 SQLite 自动建。
CREATE INDEX IF NOT EXISTS idx_demo_item_name ON demo_item(name);
";

/// 打开（必要时创建）数据库并确保表结构就绪。
///
/// `path` 传文件路径。传 `":memory:"` 可得到一个内存库（测试用，
/// 但要注意：内存库随连接销毁而消失，**不适合**验证"数据真的落盘了"）。
pub fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path).map_err(AppError::DbOpen)?;
    configure(&conn)?;
    conn.execute_batch(SCHEMA).map_err(AppError::Db)?;
    Ok(conn)
}

/// 连接级 PRAGMA。每一项都对应一个真实会踩到的问题。
fn configure(conn: &Connection) -> Result<(), AppError> {
    // 单进程桌面应用里，锁等待几乎只来自"后台写 + UI 读"的瞬时重叠。
    // 给 5 秒等待，避免直接抛 SQLITE_BUSY 到用户脸上。
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(AppError::Db)?;

    // WAL：读写不互相阻塞。注意内存库不支持 WAL —— SQLite 会静默保持原模式，
    // 不报错，所以这里无需分支。
    // foreign_keys 默认是关的（SQLite 的历史包袱），显式打开。
    // 本模板暂无外键，但把这条基线定下来，将来加关联表就不用再想起来改。
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(AppError::Db)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_is_idempotent_and_creates_schema() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("t.db");

        // 故意开两次：模拟"第一次启动建库，第二次启动复用"。
        {
            let conn = open(&path).expect("首次打开");
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM demo_item", [], |r| r.get(0))
                .expect("表应已存在");
            assert_eq!(n, 0);
        }
        {
            let conn = open(&path).expect("再次打开不应失败（IF NOT EXISTS）");
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM demo_item", [], |r| r.get(0))
                .expect("表仍在");
            assert_eq!(n, 0);
        }
    }

    #[test]
    fn open_reports_database_error_on_unwritable_path() {
        let dir = tempfile::tempdir().expect("临时目录");
        // 把目录本身当文件打开 —— 必然失败。
        let err = open(dir.path()).expect_err("打开目录应当失败");
        assert_eq!(err.code(), "E_DB_OPEN");
    }
}
