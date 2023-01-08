use serde::Serialize;
use tonic::Code;

use crate::db::DbError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    InternalServerError(anyhow::Error),
    #[error("resource not found")]
    NotFound,
    #[error("{source}")]
    WithBody {
        message: serde_json::Value,
        #[source]
        source: Box<Self>,
    },
}

impl Error {
    pub fn with_body<S: Serialize>(self, s: S) -> Result<Self, Self> {
        Ok(Self::WithBody {
            message: serde_json::to_value(s).map_err(|e| Self::InternalServerError(e.into()))?,
            source: Box::new(self),
        })
    }
}

impl Error {
    pub fn status_code(&self) -> Code {
        match self {
            Error::InternalServerError(_) => Code::Internal,
            Error::NotFound => Code::NotFound,
            Error::WithBody { message: _, source } => source.status_code(),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(error: Error) -> tonic::Status {
        tracing::info!("error response: {:#}", error);
        if let Error::WithBody { message, source } = error {
            tonic::Status::new(source.status_code(), message.to_string())
        } else {
            tonic::Status::new(error.status_code(), "")
        }
    }
}

impl From<DbError> for Error {
    fn from(e: DbError) -> Self {
        Error::InternalServerError(anyhow::Error::from(e).context("db error"))
    }
}
