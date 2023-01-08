use deadpool_diesel::postgres::Pool;
use diesel::prelude::*;
use url::Url;
use uuid::Uuid;

use crate::db::DbError;
use crate::schema::voice_servers;

#[derive(Insertable, Clone)]
#[diesel(table_name = voice_servers)]
pub struct NewVoiceServer {
    host: String,
}

impl NewVoiceServer {
    pub fn new(host: Url) -> Self {
        Self { host: host.into() }
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
            .returning(id)
            .get_result(conn)?;

        Ok(res)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Selectable, Identifiable, Queryable)]
#[diesel(table_name = voice_servers)]
pub struct VoiceServer {
    pub id: Uuid,
    pub host: String,
}

impl VoiceServer {
    pub async fn get(id: Uuid, conn: &Pool) -> Result<Option<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(move |conn| Self::do_get(conn, id)).await??;
        Ok(res)
    }

    pub async fn get_smallest_load(conn: &Pool) -> Result<Option<(Self, i64)>, DbError> {
        let conn = conn.get().await?;
        conn.interact(move |conn| Self::do_get_smallest_load(conn))
            .await?
    }

    pub fn do_get_smallest_load(conn: &mut PgConnection) -> Result<Option<(Self, i64)>, DbError> {
        use crate::schema::*;
        use diesel::dsl::count;
        let res = voice_servers::table
            .left_join(channels::table)
            .group_by(voice_servers::id)
            .select((Self::as_select(), count(channels::id.nullable())))
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
