use structopt::StructOpt;
use tracing::Level;

use train::{Client, Opt};

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    match opt.verbose() {
        0 => (),
        1 => tracing_subscriber::fmt().with_max_level(Level::INFO).init(),
        2 => tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .init(),
        _ => tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .init(),
    }

    let client = Client::new(opt)?;
    client.download()?;

    Ok(())
}
