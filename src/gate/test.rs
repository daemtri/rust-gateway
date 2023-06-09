use futures::future::BoxFuture;
use std::future::Future;
use std::io::Result;

pub trait Tester {
    type Future: Future<Output = Result<()>>;

    fn test(&self) -> Self::Future;
}

impl Tester for String {
    type Future = BoxFuture<'static, Result<()>>;

    fn test(&self) -> Self::Future {
        Box::pin(async move { Ok(()) })
    }
}

async fn test() -> Result<()> {
    let s = String::from("hello");
    s.test().await
}
