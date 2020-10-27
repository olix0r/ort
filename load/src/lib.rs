#![deny(warnings, rust_2018_idioms)]

use structopt::StructOpt;

#[derive(Clone, Debug, StructOpt)]
#[structopt(about = "Load generator")]
pub struct Load {}

impl Load {
    pub async fn run(self) {}
}
