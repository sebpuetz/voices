use deadpool_diesel::postgres::Pool;
use deadpool_diesel::{Manager, Runtime};
use diesel::migration::MigrationSource;
use diesel::pg::Pg;
use diesel::{Connection, PgConnection, RunQueryDsl};
use diesel_migrations::MigrationHarness;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct PoolGuard {
    pool: Pool,
    database_url: String,
    id: Uuid,
}

impl PoolGuard {
    pub async fn setup(
        migrations: impl MigrationSource<Pg> + Send + Sync + 'static,
    ) -> anyhow::Result<PoolGuard> {
        dotenvy::dotenv()?;
        let id = Uuid::new_v4();
        let base_database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432".into());
        let mut url = Url::try_from(base_database_url.as_str()).unwrap();
        url.set_path("");
        let base_database_url = url.to_string();
        let base_database_url_ = base_database_url.clone();
        let test_database_url = tokio::task::spawn_blocking(move || {
            let mut conn = PgConnection::establish(&base_database_url_)?;
            diesel::sql_query(format!(r#"CREATE DATABASE "{}";"#, id)).execute(&mut conn)?;

            let database_url = format!("{}/{}", base_database_url_, id);
            Ok::<_, anyhow::Error>(database_url)
        })
        .await??;
        let manager = Manager::new(test_database_url, Runtime::Tokio1);
        let pool = Pool::builder(manager).build()?;
        let obj = pool.get().await?;
        obj.interact(move |conn| {
            conn.run_pending_migrations(migrations)
                .map_err(|e| anyhow::anyhow!("migration failed {}", e))?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("Migrations failed"))??;
        Ok(PoolGuard {
            pool,
            database_url: base_database_url,
            id,
        })
    }
}

impl std::ops::DerefMut for PoolGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool
    }
}

impl std::ops::Deref for PoolGuard {
    type Target = Pool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        self.pool.close();
        let id = self.id;
        let db = self.database_url.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = PgConnection::establish(&db).unwrap();
            diesel::sql_query(format!(r#"DROP DATABASE IF EXISTS "{}" WITH(FORCE);"#, id))
                .execute(&mut conn)
                .expect("failed to create test db");
        });
    }
}
