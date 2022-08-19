use std::future::Future;
use std::time::Duration;

use tokio::time::Timeout;

pub trait TimeoutExt<Fut> {
    fn timeout(&self, fut: Fut) -> Timeout<Fut>;
}

impl<Fut> TimeoutExt<Fut> for Duration
where
    Fut: Future,
{
    fn timeout(&self, fut: Fut) -> Timeout<Fut> {
        tokio::time::timeout(*self, fut)
    }
}
