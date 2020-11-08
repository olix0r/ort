#![deny(warnings, rust_2018_idioms)]

#[cfg(feature = "load")]
use ort_load as load;
#[cfg(feature = "server")]
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
    #[cfg(feature = "load")]
    Load(load::Opt),

    #[cfg(feature = "server")]
    Server(server::Server),
}

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let Ort { threads, cmd } = Ort::from_args();

    let threads = threads.unwrap_or_else(num_cpus::get);
    let mut rt = {
        let mut rt = rt::Builder::new();
        rt.threaded_scheduler();
        rt.enable_all();
        rt.core_threads(threads);
        rt.build()?
    };

    #[cfg(feature = "load")]
    if let Cmd::Load(l) = cmd {
        return rt.block_on(l.run(threads));
    }

    #[cfg(feature = "server")]
    if let Cmd::Server(s) = cmd {
        return rt.block_on(s.run());
    }

    Ok(())
}
