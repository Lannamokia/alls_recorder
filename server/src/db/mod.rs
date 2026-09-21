//! 数据库抽象层：同时支持 PostgreSQL（多机/既有部署）与 SQLite（单机零配置部署）。
//!
//! - `DbPool` 枚举包装两种连接池；
//! - SQL 文本统一按 PostgreSQL 方言编写（`$N` 占位符），SQLite 分支由
//!   `sqlite_sql()` 在运行时将 `$N` 翻译为 SQLite 原生的 `?N`；
//! - 方言差异仅限 schema 脚本与极少量查询，查询调用点通过 `dbq!` / `dbq_as!` /
//!   `dbq_scalar!` 宏双分支执行。

use sqlx::{PgPool, SqlitePool};

const SCHEMA_PG_SQL: &str = include_str!("../../schema.sql");
const SCHEMA_SQLITE_SQL: &str = include_str!("../../schema_sqlite.sql");

#[derive(Clone)]
pub enum DbPool {
    Pg(PgPool),
    Sqlite(SqlitePool),
}

impl DbPool {
    pub fn is_pg(&self) -> bool {
        matches!(self, DbPool::Pg(_))
    }
}

/// 将 PostgreSQL 占位符 `$N` 翻译为 SQLite 占位符 `?N`。
/// SQLite 原生支持 `?N` 编号参数，且本项目所有查询的绑定顺序与编号一致，
/// 因此纯字符替换即可保证语义等价。
pub fn sqlite_sql(sql: &str) -> String {
    sql.replace('$', "?")
}

/// 按 URL 前缀创建连接池并初始化 schema。
/// `sqlite://path`（或裸路径）走 SQLite，`postgres://` 走 PostgreSQL。
pub async fn connect(url: &str) -> Result<DbPool, sqlx::Error> {
    if is_sqlite_url(url) {
        let path = sqlite_path_from_url(url);
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePool::connect_with(options).await?;
        ensure_schema_sqlite(&pool).await?;
        Ok(DbPool::Sqlite(pool))
    } else {
        let pool = PgPool::connect(url).await?;
        ensure_schema_pg(&pool).await?;
        Ok(DbPool::Pg(pool))
    }
}

pub fn is_sqlite_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("sqlite:") || !lower.contains("://")
}

/// 从 `sqlite://relative/or/abs/path` 提取文件路径。
pub fn sqlite_path_from_url(url: &str) -> String {
    let stripped = url.strip_prefix("sqlite://").unwrap_or(url);
    // 允许 sqlite:relative.db 形式
    let stripped = stripped.strip_prefix("sqlite:").unwrap_or(stripped);
    stripped.to_string()
}

/// 确保连接存活（用于服务模式的断线重连探测）。SQLite 文件库不会"断线"，恒返回 true。
pub async fn ping(pool: &DbPool) -> Result<(), sqlx::Error> {
    match pool {
        DbPool::Pg(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
        DbPool::Sqlite(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
    }
}

pub async fn ensure_schema_pg(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA_PG_SQL).execute(pool).await?;
    Ok(())
}

pub async fn ensure_schema_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA_SQLITE_SQL).execute(pool).await?;
    Ok(())
}

/// 事务抽象：与 DbPool 对应的双分支事务。
pub enum DbTx<'c> {
    Pg(sqlx::Transaction<'c, sqlx::Postgres>),
    Sqlite(sqlx::Transaction<'c, sqlx::Sqlite>),
}

impl<'c> DbTx<'c> {
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        match self {
            DbTx::Pg(t) => t.commit().await,
            DbTx::Sqlite(t) => t.commit().await,
        }
    }
}

impl DbPool {
    pub async fn begin(&self) -> Result<DbTx<'_>, sqlx::Error> {
        match self {
            DbPool::Pg(p) => p.begin().await.map(DbTx::Pg),
            DbPool::Sqlite(p) => p.begin().await.map(DbTx::Sqlite),
        }
    }
}

/// 事务内动态查询：`dbq_tx!(tx, execute, "SQL", [a, b])`
#[macro_export]
macro_rules! dbq_tx {
    ($tx:expr, execute, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $tx {
            $crate::db::DbTx::Pg(__t) =>
                sqlx::query($sql) $(.bind($bind))* .execute(&mut **__t).await.map(|r| r.rows_affected()),
            $crate::db::DbTx::Sqlite(__t) =>
                sqlx::query(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .execute(&mut **__t).await.map(|r| r.rows_affected()),
        }
    }};
    ($tx:expr, $fetch:ident, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $tx {
            $crate::db::DbTx::Pg(__t) =>
                sqlx::query($sql) $(.bind($bind))* .$fetch(&mut **__t).await,
            $crate::db::DbTx::Sqlite(__t) =>
                sqlx::query(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .$fetch(&mut **__t).await,
        }
    }};
}
/// 动态查询（无行映射）：
/// * `dbq!(pool, execute, "SQL $1, $2", [a, b])` → `Result<u64, _>`（rows_affected）
/// * `dbq!(pool, exists, "SQL $1", [a])` → `Result<bool, _>`（是否存在匹配行）
#[macro_export]
macro_rules! dbq {
    ($pool:expr, execute, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $pool {
            $crate::db::DbPool::Pg(__p) =>
                sqlx::query($sql) $(.bind($bind))* .execute(__p).await.map(|r| r.rows_affected()),
            $crate::db::DbPool::Sqlite(__p) =>
                sqlx::query(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .execute(__p).await.map(|r| r.rows_affected()),
        }
    }};
    ($pool:expr, exists, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $pool {
            $crate::db::DbPool::Pg(__p) =>
                sqlx::query($sql) $(.bind($bind))* .fetch_optional(__p).await.map(|r| r.is_some()),
            $crate::db::DbPool::Sqlite(__p) =>
                sqlx::query(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .fetch_optional(__p).await.map(|r| r.is_some()),
        }
    }};
}

/// 行映射查询（query_as）：`dbq_as!(pool, User, fetch_optional, "SQL", [a])`
#[macro_export]
macro_rules! dbq_as {
    ($pool:expr, $ty:ty, $fetch:ident, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $pool {
            $crate::db::DbPool::Pg(__p) =>
                sqlx::query_as::<_, $ty>($sql) $(.bind($bind))* .$fetch(__p).await,
            $crate::db::DbPool::Sqlite(__p) =>
                sqlx::query_as::<_, $ty>(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .$fetch(__p).await,
        }
    }};
}

/// 单值查询（query_scalar）：`dbq_scalar!(pool, Uuid, fetch_one, "SQL", [a])`
#[macro_export]
macro_rules! dbq_scalar {
    ($pool:expr, $ty:ty, $fetch:ident, $sql:expr, [$($bind:expr),* $(,)?]) => {{
        match $pool {
            $crate::db::DbPool::Pg(__p) =>
                sqlx::query_scalar::<_, $ty>($sql) $(.bind($bind))* .$fetch(__p).await,
            $crate::db::DbPool::Sqlite(__p) =>
                sqlx::query_scalar::<_, $ty>(&$crate::db::sqlite_sql($sql)) $(.bind($bind))* .$fetch(__p).await,
        }
    }};
}
