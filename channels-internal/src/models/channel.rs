use chrono::{DateTime, Utc};
use deadpool_diesel::postgres::Pool;
use diesel::prelude::*;
use diesel::{Associations, Identifiable, Insertable, Queryable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::server::Server;
use super::voice_server::VoiceServer;
use crate::db::DbError;
use crate::schema::channels;

#[derive(Debug, Insertable, Clone, Deserialize, Serialize)]
#[diesel(table_name = channels)]
pub struct NewChannel {
    server_id: Uuid,
    name: String,
}

impl NewChannel {
    pub fn new(server_id: Uuid, name: String) -> Self {
        Self { server_id, name }
    }

    pub async fn create(&self, conn: &Pool) -> Result<Uuid, DbError> {
        let conn = conn.get().await?;
        let slf = self.clone();
        let res = conn.interact(move |conn| slf.do_create(conn)).await??;
        Ok(res)
    }

    fn do_create(&self, conn: &mut PgConnection) -> Result<Uuid, DbError> {
        use crate::schema::channels::dsl::*;

        let res = diesel::insert_into(channels)
            .values(self)
            .returning(id)
            .get_result(conn)?;

        Ok(res)
    }
}

#[derive(Clone, Identifiable, Queryable, Selectable, Associations, PartialEq, Debug, Serialize)]
#[diesel(
    table_name = channels,
    belongs_to(Server, foreign_key = server_id),
    belongs_to(VoiceServer, foreign_key = assigned_to))]
pub struct Channel {
    pub id: Uuid,
    pub server_id: Uuid,
    pub assigned_to: Option<Uuid>,
    pub name: String,
    pub updated_at: DateTime<Utc>,
}

impl Channel {
    pub async fn get(id: Uuid, conn: &Pool) -> Result<Option<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(move |conn| Self::do_get(conn, id)).await??;
        Ok(res)
    }

    fn do_get(conn: &mut PgConnection, channel_id: Uuid) -> Result<Option<Self>, DbError> {
        use crate::schema::channels::dsl::*;

        Ok(channels
            .select(Self::as_select())
            .filter(id.eq(channel_id))
            .get_result(conn)
            .optional()?)
    }

    pub async fn delete(cid: Uuid, conn: Pool) -> Result<bool, DbError> {
        let conn = conn.get().await?;
        conn.interact(move |conn| {
            use crate::schema::channels::dsl::*;
            Ok(diesel::delete(channels.filter(id.eq(cid)))
                .execute(conn)
                .optional()?
                .is_some())
        })
        .await?
    }

    pub async fn assign_to(
        cid: Uuid,
        sid: Uuid,
        reassign: bool,
        pool: &Pool,
    ) -> Result<Option<Self>, DbError> {
        tracing::info!(reassign, "assigning channel {} to voice srv {}", cid, sid);
        let conn = pool.get().await?;
        let res = conn
            .interact(move |conn| Self::do_assign_to(cid, sid, reassign, conn))
            .await?;
        res
    }

    fn do_assign_to(
        cid: Uuid,
        sid: Uuid,
        reassign: bool,
        conn: &mut PgConnection,
    ) -> Result<Option<Self>, DbError> {
        use crate::schema::channels::dsl::*;
        conn.transaction(|conn| {
            let c = match Self::do_get(conn, cid)? {
                Some(c) => c,
                None => return Ok(None),
            };
            if c.assigned_to.is_some() && !reassign {
                return Ok(Some(c));
            }
            Ok(diesel::update(channels.filter(id.eq(cid)))
                .set(assigned_to.eq(sid))
                .returning(Self::as_select())
                .get_result(conn)
                .optional()?)
        })
    }

    pub async fn unassign(cid: Uuid, pool: &Pool) -> Result<(), DbError> {
        let conn = pool.get().await?;
        let res = conn
            .interact(move |conn| {
                use crate::schema::channels::dsl::*;

                diesel::update(channels.filter(id.eq(cid)))
                    .set(assigned_to.eq(None::<Uuid>))
                    .returning(assigned_to.nullable())
                    .execute(conn)
            })
            .await??;
        if res == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
}
