use deadpool_diesel::postgres::Pool;

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error(transparent)]
    PoolError(#[from] deadpool::managed::PoolError<deadpool_diesel::Error>),
    #[error("{0}")]
    InteractError(&'static str),
    #[error(transparent)]
    DieselError(#[from] diesel::result::Error),
}

impl From<deadpool_diesel::postgres::InteractError> for DbError {
    fn from(e: deadpool_diesel::postgres::InteractError) -> Self {
        match e {
            deadpool_diesel::InteractError::Panic(_) => {
                DbError::InteractError("db interaction panic")
            }
            deadpool_diesel::InteractError::Aborted => {
                DbError::InteractError("db interaction aborted")
            }
        }
    }
}

use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::*;
use diesel::query_dsl::methods::LoadQuery;
use diesel::sql_types::BigInt;

pub trait Paginate: Sized {
    fn paginate(self, page: i64) -> Paginated<Self>;
}

impl<T> Paginate for T {
    fn paginate(self, page: i64) -> Paginated<Self> {
        Paginated {
            query: self,
            per_page: DEFAULT_PER_PAGE,
            page,
            offset: (page - 1) * DEFAULT_PER_PAGE,
        }
    }
}

const DEFAULT_PER_PAGE: i64 = 10;

#[derive(Debug, Clone, Copy, QueryId)]
pub struct Paginated<T> {
    query: T,
    page: i64,
    per_page: i64,
    offset: i64,
}

impl<T> Paginated<T> {
    pub fn per_page(self, per_page: i64) -> Self {
        Paginated {
            per_page,
            offset: (self.page - 1) * per_page,
            ..self
        }
    }

    pub async fn load_and_count_pages<'a, U>(self, conn: &Pool) -> Result<(Vec<U>, i64), DbError>
    where
        Self: LoadQuery<'a, PgConnection, (U, i64)> + Send + 'static,
        U: Send + Sync + 'static,
    {
        let obj = conn.get().await?;
        obj.interact(move |conn| {
            let per_page = self.per_page;
            let results = self.load::<(U, i64)>(conn)?;
            let total = results.get(0).map(|x| x.1).unwrap_or(0);
            let records = results.into_iter().map(|x| x.0).collect();
            let total_pages = (total as f64 / per_page as f64).ceil() as i64;
            Ok((records, total_pages))
        })
        .await?
    }
}

impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

impl<T> RunQueryDsl<PgConnection> for Paginated<T> {}

impl<T> QueryFragment<Pg> for Paginated<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("SELECT *, COUNT(*) OVER () FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") t LIMIT ");
        out.push_bind_param::<BigInt, _>(&self.per_page)?;
        out.push_sql(" OFFSET ");
        out.push_bind_param::<BigInt, _>(&self.offset)?;
        Ok(())
    }
}
