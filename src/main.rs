#![deny(warnings, rust_2018_idioms)]

use ortiofay_controller as controller;
use ortiofay_load as load;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about = "Load harness")]
enum Ortiofay {
    Controller(controller::Controller),
    Load(load::Load),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    match Ortiofay::from_args() {
        Ortiofay::Controller(c) => c.run().await?,
        Ortiofay::Load(l) => l.run().await,
    }

    Ok(())
}
