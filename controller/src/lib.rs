pub mod bench;

pub use self::bench::Bench;
use futures::prelude::*;
use indexmap::IndexMap;
use k8s_openapi::api::core::v1::{Pod, Service};
use std::sync::Arc;
use structopt::StructOpt;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(StructOpt)]
#[structopt(about = "Kubernetes controller")]
pub struct Controller {
    #[structopt(short, long, name = "NAMESPACE", default_value = "default")]
    namespace: String,
}

struct Ctx {
    //client: kube::Client,
    state: Mutex<State>,
}

struct State {
    active: IndexMap<String, Bench>,
}

impl Controller {
    pub async fn run(self) -> Result<(), kube::Error> {
        let Self { namespace } = self;
        let client = kube::Client::try_default().await?;

        let benches_api = kube::Api::<Bench>::namespaced(client.clone(), &namespace);
        let _svc_api = kube::Api::<Service>::namespaced(client.clone(), &namespace);
        let _pods_api = kube::Api::<Pod>::namespaced(client.clone(), &namespace);

        let ctx = Arc::new(Ctx {
            //client,
            state: Mutex::new(State {
                active: IndexMap::default(),
            }),
        });

        let mut revision = "0".to_string();
        loop {
            let benches_params = kube::api::ListParams::default();
            info!(%revision, "Watching benches");
            let mut benches_stream = benches_api.watch(&benches_params, &revision).await?.boxed();
            while let Some(ev) = benches_stream.next().await {
                match ev {
                    Err(error) => {
                        warn!(?error);
                    }
                    Ok(kube::api::WatchEvent::Added(bench)) => {
                        if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                            revision = rv;
                        }
                        let name = kube::api::Meta::name(&bench);
                        let mut state = ctx.state.lock().await;
                        state.active.insert(name.clone(), bench);
                        info!(%name, %revision, active = %state.active.len(), "Added");
                    }
                    Ok(kube::api::WatchEvent::Modified(bench)) => {
                        if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                            revision = rv;
                        }
                        let name = kube::api::Meta::name(&bench);
                        let mut state = ctx.state.lock().await;
                        state.active.insert(name.clone(), bench);
                        info!(%name, %revision, active = %state.active.len(), "Modified");
                    }
                    Ok(kube::api::WatchEvent::Deleted(bench)) => {
                        if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                            revision = rv;
                        }
                        let name = kube::api::Meta::name(&bench);
                        let mut state = ctx.state.lock().await;
                        state.active.remove(&name);
                        info!(%name, %revision, active = %state.active.len(), "Deleted");
                    }
                    Ok(kube::api::WatchEvent::Bookmark(b)) => {
                        revision = b.metadata.resource_version;
                    }
                    Ok(kube::api::WatchEvent::Error(error)) => {
                        warn!(%error);
                        break;
                    }
                }
            }
            info!("Stream completed");
        }
    }
}
