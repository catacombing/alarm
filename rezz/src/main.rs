use tracing::{subscriber, Level};
use tracing_subscriber::FmtSubscriber;

mod dbus;
mod logind;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Setup logging.
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    dbus::launch().await;
}
