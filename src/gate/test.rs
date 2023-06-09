use futures::future::BoxFuture;
use std::future::Future;
use std::io::Result;

pub trait Tester {
    type Future<'a>: Future<Output = Result<()>>
    where
        Self: 'a;

    fn test(&self) -> Self::Future<'_>;
}

impl Tester for String {
    type Future<'a> = BoxFuture<'a, Result<()>>;

    fn test(&self) -> Self::Future<'_> {
        Box::pin(async move { Ok(()) })
    }
}

async fn test() -> Result<()> {
    let s = String::from("hello");
    s.test().await
}
