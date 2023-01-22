use chrono::{DateTime, Utc};
use deadpool_diesel::postgres::Pool;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::DbError;
use crate::schema::voice_servers;

#[derive(Insertable, Clone)]
#[diesel(table_name = voice_servers)]
pub struct NewVoiceServer {
    id: Uuid,
    host_url: String,
}

impl NewVoiceServer {
    pub fn new(id: Uuid, host_url: String) -> Self {
        Self { id, host_url }
    }

    pub async fn create_or_update(&self, conn: &Pool) -> Result<Uuid, DbError> {
        let conn = conn.get().await?;
        let slf = self.clone();
        let res = conn.interact(move |conn| slf.do_create(conn)).await??;
        Ok(res)
    }

    fn do_create(&self, conn: &mut PgConnection) -> Result<Uuid, DbError> {
        use crate::schema::voice_servers::dsl::*;

        let res = diesel::insert_into(voice_servers)
            .values(self)
            .on_conflict(id)
            .do_update()
            .set(last_seen.eq(Utc::now()))
            .returning(id)
            .get_result(conn)?;

        Ok(res)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Selectable, Identifiable, Queryable)]
#[diesel(table_name = voice_servers)]
pub struct VoiceServer {
    pub id: Uuid,
    pub host_url: String,
    pub last_seen: DateTime<Utc>,
}

impl VoiceServer {
    pub async fn cleanup_stale(conn: &Pool) -> Result<Vec<Uuid>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(Self::do_cleanup_stale).await??;
        Ok(res)
    }

    fn do_cleanup_stale(conn: &mut PgConnection) -> Result<Vec<Uuid>, DbError> {
        use crate::schema::voice_servers::dsl::*;

        diesel::delete(
            voice_servers.filter(last_seen.lt(Utc::now() - chrono::Duration::seconds(35))),
        )
        .returning(id)
        .get_results(conn)
        .map_err(Into::into)
    }

    pub async fn get_active(id: Uuid, conn: &Pool) -> Result<Option<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(move |conn| Self::do_get(conn, id)).await??;
        Ok(res.filter(|v| v.last_seen > Utc::now() - chrono::Duration::seconds(35)))
    }

    pub async fn get_smallest_load(conn: &Pool) -> Result<Option<(Self, i64)>, DbError> {
        let conn = conn.get().await?;
        conn.interact(Self::do_get_smallest_load).await?
    }

    pub fn do_get_smallest_load(conn: &mut PgConnection) -> Result<Option<(Self, i64)>, DbError> {
        use crate::schema::*;
        use diesel::dsl::count;
        let res = voice_servers::table
            .left_join(channels::table)
            .group_by(voice_servers::id)
            .select((Self::as_select(), count(channels::id.nullable())))
            .filter(voice_servers::last_seen.gt(Utc::now() - chrono::Duration::seconds(35)))
            .order_by(count(channels::id).asc())
            .limit(1)
            .get_result::<(Self, i64)>(conn)
            .optional()?;
        Ok(res)
    }

    fn do_get(conn: &mut PgConnection, serv_id: Uuid) -> Result<Option<Self>, DbError> {
        use crate::schema::voice_servers::dsl::*;

        Ok(voice_servers
            .select(Self::as_select())
            .filter(id.eq(serv_id))
            .get_result(conn)
            .optional()?)
    }
}