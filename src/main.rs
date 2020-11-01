#![deny(warnings, rust_2018_idioms)]

#[cfg(feature = "load")]
use ort_load as load;
#[cfg(feature = "server")]
use ort_server as server;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about = "Load harness")]
enum Ort {
    #[cfg(feature = "load")]
    Load(load::Load),

    #[cfg(feature = "server")]
    Server(server::Server),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let cmd = Ort::from_args();

    #[cfg(feature = "load")]
    if let Ort::Load(l) = cmd {
        l.run().await?;
        return Ok(());
    }

    #[cfg(feature = "server")]
    if let Ort::Server(s) = cmd {
        s.run().await?;
        return Ok(());
    }

    Ok(())
}
