use std::future::Future;
use std::time::Duration;

use tokio::time::{Sleep, Timeout};

pub trait TimeoutExt<Fut> {
    fn timeout(&self, fut: Fut) -> Timeout<Fut>;
    fn sleep(&self) -> Sleep;
}

impl<Fut> TimeoutExt<Fut> for Duration
where
    Fut: Future,
{
    fn timeout(&self, fut: Fut) -> Timeout<Fut> {
        tokio::time::timeout(*self, fut)
    }
    fn sleep(&self) -> Sleep {
        tokio::time::sleep(*self)
    }
}
