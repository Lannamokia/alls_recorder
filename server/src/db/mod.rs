use sqlx::PgPool;

const SCHEMA_SQL: &str = include_str!("../../schema.sql");

/// Run the idempotent schema script to ensure all tables and columns exist.
/// Safe to call on every startup — uses CREATE IF NOT EXISTS / ADD COLUMN IF NOT EXISTS.
pub async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await?;
    Ok(())
}
