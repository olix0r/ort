#![deny(warnings, rust_2018_idioms)]

#[cfg(feature = "controller")]
use ortiofay_controller as controller;
#[cfg(feature = "load")]
use ortiofay_load as load;
#[cfg(feature = "server")]
use ortiofay_server as server;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about = "Load harness")]
enum Ortiofay {
    #[cfg(feature = "controller")]
    Controller(controller::Controller),

    #[cfg(feature = "load")]
    Load(load::Load),

    #[cfg(feature = "server")]
    Server(server::Server),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let cmd = Ortiofay::from_args();

    #[cfg(feature = "controller")]
    if let Ortiofay::Controller(c) = cmd {
        c.run().await?;
        return Ok(());
    }

    #[cfg(feature = "load")]
    if let Ortiofay::Load(l) = cmd {
        l.run().await?;
        return Ok(());
    }

    #[cfg(feature = "server")]
    if let Ortiofay::Server(s) = cmd {
        s.run().await?;
        return Ok(());
    }

    Ok(())
}
