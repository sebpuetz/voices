use deadpool_diesel::postgres::Pool;
use diesel::helper_types::AsSelect;
use diesel::helper_types::SqlTypeOf;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::Insertable;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::db::DbError;
use crate::schema::servers::BoxedQuery;
use crate::schema::*;

use super::channel::ChannelDb;

#[derive(Insertable, Clone)]
#[diesel(table_name = servers)]
pub struct NewServer {
    name: String,
}

impl NewServer {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub async fn create(&self, conn: &Pool) -> Result<Uuid, DbError> {
        let conn = conn.get().await?;
        let slf = self.clone();
        let res = conn.interact(move |conn| slf.do_create(conn)).await??;
        Ok(res)
    }

    fn do_create(&self, conn: &mut PgConnection) -> Result<Uuid, DbError> {
        use crate::schema::servers::dsl::*;

        let res = diesel::insert_into(servers)
            .values(self)
            .returning(id)
            .get_result(conn)?;

        Ok(res)
    }
}

#[derive(
    Clone, PartialEq, Eq, Debug, Selectable, Identifiable, Queryable, Serialize, Deserialize,
)]
#[diesel(table_name = servers)]
pub struct ServerDb {
    pub id: Uuid,
    pub name: String,
}

impl ServerDb {
    // pub fn by_name<'a, Db>(q: String) -> BoxedQuery<'a, Db, SqlTypeOf<AsSelect<Server, Db>>>
    // where
    //     Db: Backend + Send,
    // {
    //     use crate::schema::servers;
    //     servers::table
    //         .select(Self::as_select())
    //         .filter(servers::columns::name.eq(q))
    //         .into_boxed()
    // }

    pub async fn get_many(offset: usize, limit: usize, conn: &Pool) -> Result<Vec<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn
            .interact(move |conn| Self::do_get_many(offset, limit).load(conn))
            .await??;
        Ok(res)
    }

    // pub async fn update(target: Uuid, name: String, conn: &Pool) -> Result<(), DbError> {
    //     let conn = conn.get().await?;
    //     let res = conn.interact(move |conn| Ok::<_, DbError>(())).await??;
    //     Ok(())
    // }

    fn do_get_many(
        offset: usize,
        limit: usize,
        // conn: &mut PgConnection,
    ) -> BoxedQuery<'static, Pg, SqlTypeOf<AsSelect<ServerDb, Pg>>> {
        Self::all().offset(offset as _).limit(limit as _)
    }

    pub fn all() -> BoxedQuery<'static, Pg, SqlTypeOf<AsSelect<ServerDb, Pg>>> {
        servers::table.select(Self::as_select()).into_boxed()
    }

    pub async fn load_all(conn: &Pool) -> Result<Vec<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(move |conn| Self::all().load(conn)).await??;
        Ok(res)
    }

    pub async fn get(id: Uuid, conn: &Pool) -> Result<Option<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn.interact(move |conn| Self::do_get(conn, id)).await??;
        Ok(res)
    }

    fn do_get(conn: &mut PgConnection, server_id: Uuid) -> Result<Option<Self>, DbError> {
        use crate::schema::servers::dsl::*;

        Ok(servers
            .select(Self::as_select())
            .filter(id.eq(server_id))
            .get_result(conn)
            .optional()?)
    }

    pub async fn get_members_by_id(id: Uuid, conn: &Pool) -> Result<Vec<Uuid>, DbError> {
        let conn = conn.get().await?;
        let res = conn
            .interact(move |conn| Self::do_get_members(conn, id))
            .await??;
        Ok(res)
    }

    pub async fn get_members(&self, conn: &Pool) -> Result<Vec<Uuid>, DbError> {
        Self::get_members_by_id(self.id, conn).await
    }

    fn do_get_members(conn: &mut PgConnection, server_id: Uuid) -> Result<Vec<Uuid>, DbError> {
        servers_members::table
            .select(servers_members::columns::client_id)
            .filter(servers_members::columns::server_id.eq(server_id))
            .load(conn)
            .map_err(Into::into)
    }

    pub async fn join_by_id(id: Uuid, member_id: Uuid, conn: &Pool) -> Result<(), DbError> {
        let conn = conn.get().await?;
        conn.interact(move |conn| Self::do_join_by_id(conn, id, member_id))
            .await??;
        Ok(())
    }

    fn do_join_by_id(
        conn: &mut PgConnection,
        id: Uuid,
        join_member_id: Uuid,
    ) -> Result<(), DbError> {
        use crate::schema::servers_members::dsl::*;
        diesel::insert_into(servers_members)
            .values((server_id.eq(id), client_id.eq(join_member_id)))
            .execute(conn)?;
        Ok(())
    }

    pub async fn get_channels(&self, conn: &Pool) -> Result<Vec<ChannelDb>, DbError> {
        let conn = conn.get().await?;
        let slf = self.clone();
        let res = conn
            .interact(move |conn| ChannelDb::belonging_to(&slf).get_results(conn))
            .await??;
        Ok(res)
    }
}

// FIXME: remove?
#[cfg(test)]
#[derive(Clone)]
pub struct ServerWithChannels {
    // #[diesel(embed)]
    pub server: ServerDb,
    pub channels: Vec<ChannelDb>,
}
#[cfg(test)]
impl ServerWithChannels {
    // pub fn new(server: ServerDb, channels: Vec<ChannelDb>) -> Self {
    //     Self { server, channels }
    // }

    // pub async fn update(&self, conn: &Pool) -> Result<(), DbError> {
    //     let conn = conn.get().await?;
    //     let slf = self.clone();
    //     conn.interact(move |conn| {
    //         conn.transaction(|conn| {
    //             {
    //                 use crate::schema::servers::dsl::*;
    //                 diesel::update(servers.filter(id.eq(slf.server.id)))
    //                     .set(name.eq(slf.server.name))
    //                     .execute(conn)?;
    //             }
    //             {
    //                 use crate::schema::channels::dsl::*;
    //                 for channel in slf.channels {
    //                     diesel::update(channels.filter(id.eq(channel.id)))
    //                         .set(name.eq(channel.name))
    //                         .execute(conn)?;
    //                 }
    //             }
    //             Ok(())
    //         })
    //     })
    //     .await?
    // }

    pub async fn get(server_id: Uuid, conn: &Pool) -> Result<Option<Self>, DbError> {
        let conn = conn.get().await?;
        let res = conn
            .interact(move |conn| Self::do_get(conn, server_id))
            .await??;
        Ok(res)
    }

    fn do_get(conn: &mut PgConnection, server_id: Uuid) -> Result<Option<Self>, DbError> {
        let server = ServerDb::do_get(conn, server_id)?.unwrap();
        let channels = ChannelDb::belonging_to(&server)
            .select(ChannelDb::as_select())
            .load(conn)?;
        Ok(Some(ServerWithChannels { server, channels }))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use diesel_migrations::{embed_migrations, EmbeddedMigrations};
    use uuid::Uuid;

    use crate::db_models::channel::NewChannel;
    use crate::db_models::server::ServerDb;
    use crate::test_helper::PoolGuard;

    use super::{NewServer, ServerWithChannels};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    #[tokio::test]
    async fn test() {
        dotenvy::dotenv().unwrap();
        let base_database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432".into());
        let conn = PoolGuard::setup(MIGRATIONS, base_database_url)
            .await
            .unwrap();
        let server = NewServer::new("test".into());
        let server_id = server.create(&conn).await.unwrap();
        let channel = NewChannel::new(server_id, "test".into());
        let channel_id = channel.create(&conn).await.unwrap();
        let channel_id2 = channel.create(&conn).await.unwrap();
        let ServerWithChannels { server, channels } = ServerWithChannels::get(server_id, &conn)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            ServerDb {
                id: server_id,
                name: "test".into(),
            },
            server
        );
        assert_eq!(2, channels.len());
        let channel_ids = channels
            .into_iter()
            .map(|c| c.id)
            .collect::<HashSet<Uuid>>();
        assert_eq!(
            channel_ids,
            HashSet::from_iter([channel_id, channel_id2].into_iter())
        );
        let member_id = Uuid::new_v4();
        ServerDb::join_by_id(server_id, member_id, &conn)
            .await
            .unwrap();
        let members = server.get_members(&conn).await.unwrap();
        assert_eq!(vec![member_id], members);
    }
}
