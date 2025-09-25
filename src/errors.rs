use std::{fmt::Debug, future::Future};

pub use anyhow::Ok as aok;

/// Turn -> Result into -> ()
/// Handle these non critical errors by logging.
/// These errors do not terminate, or change further control flow.
pub async fn wrap_noncritical<T, E: Debug>(f: impl Future<Output = Result<T, E>>) {
    let k = f.await;
    if let Err(e) = k {
        tracing::warn!("{:?}", e);
    }
}
