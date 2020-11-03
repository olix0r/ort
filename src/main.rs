#![deny(warnings, rust_2018_idioms)]

#[cfg(feature = "load")]
use ort_load as load;
#[cfg(feature = "server")]
use ort_server as server;
use structopt::StructOpt;
use tokio::runtime as rt;
use tracing::debug;

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
    #[cfg(feature = "load")]
    Load(load::Load),

    #[cfg(feature = "server")]
    Server(server::Server),
}

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let Ort { threads, cmd } = Ort::from_args();

    let rt = {
        let mut rt = rt::Builder::new_multi_thread();
        rt.thread_name("ort").enable_all();
        if let Some(t) = threads {
            debug!(threads = %t, "Initializing runtime");
            rt.worker_threads(t);
        }
        rt.build()?
    };

    #[cfg(feature = "load")]
    if let Cmd::Load(l) = cmd {
        return rt.block_on(l.run());
    }

    #[cfg(feature = "server")]
    if let Cmd::Server(s) = cmd {
        return rt.block_on(s.run());
    }

    Ok(())
}
