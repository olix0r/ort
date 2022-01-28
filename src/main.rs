#![deny(warnings, rust_2018_idioms)]

use clap::Parser;
use ort_load as load;
use ort_server as server;
use tokio::runtime as rt;

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Parser)]
#[clap(about = "Load harness", version)]
struct Ort {
    #[structopt(long)]
    threads: Option<usize>,

    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(Parser)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    Load(load::Cmd),
    Server(server::Cmd),
}

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let Ort { threads, cmd } = Ort::parse();

    let threads = threads.unwrap_or_else(num_cpus::get);
    let rt = rt::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(threads)
        .build()?;

    match cmd {
        Cmd::Load(l) => rt.block_on(l.run(threads))?,
        Cmd::Server(s) => rt.block_on(s.run())?,
    }

    Ok(())
}
