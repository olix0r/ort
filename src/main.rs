#![deny(warnings, rust_2018_idioms)]

use ort_load as load;
use ort_server as server;
use structopt::StructOpt;
use tokio::runtime as rt;

#[derive(StructOpt)]
#[structopt(about = "Load harness")]
struct Ort {
    #[structopt(long)]
    threads: Option<usize>,

    #[structopt(subcommand)]
    cmd: Cmd,
}

#[derive(StructOpt)]
enum Cmd {
    Load(load::Cmd),
    Server(server::Cmd),
}

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let Ort { threads, cmd } = Ort::from_args();

    let threads = threads.unwrap_or_else(num_cpus::get);
    let mut rt = rt::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(threads)
        .build()?;

    match cmd {
        Cmd::Load(l) => rt.block_on(l.run(threads)),
        Cmd::Server(s) => rt.block_on(s.run()),
    }
}
