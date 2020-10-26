#![deny(warnings, rust_2018_idioms)]
#![allow(unused_imports)]

use futures::prelude::*;
use indexmap::IndexMap;
use k8s_openapi::api::core::v1::{Pod, PodTemplateSpec, Service};
use kube::CustomResource;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(CustomResource, Clone, Debug, Deserialize, Serialize)]
#[kube(
    group = "ortiofay.olix0r.net",
    version = "v1alpha1",
    kind = "Bench",
    namespaced,
    status = "Status"
)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    load_template: PodTemplateSpec,
    server_template: PodTemplateSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Failed { message: String },
    Complete { report: Report },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    run_type: String,
    labels: String,
    num_threads: u32,
    duration_histogram: Histogram,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Histogram {
    count: u32,
    min: f32,
    max: f32,
    sum: f32,
    avg: f32,
    std_dev: f32,
    data: Vec<Bucket>,
    percentiles: Vec<Percentile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    start: f32,
    end: f32,
    percent: f32,
    count: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Percentile {
    percentile: f32,
    value: f32,
}

struct Ctx {
    //client: kube::Client,
    state: Mutex<State>,
}

struct State {
    active: IndexMap<String, Bench>,
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {
    tracing_subscriber::fmt::init();

    let ns = std::env::var("NAMESPACE")
        .ok()
        .filter(|ns| ns.is_empty())
        .unwrap_or_else(|| "default".to_string());

    let client = kube::Client::try_default().await?;

    let benches_api = kube::Api::<Bench>::namespaced(client.clone(), &ns);
    let _svc_api = kube::Api::<Service>::namespaced(client.clone(), &ns);
    let _pods_api = kube::Api::<Pod>::namespaced(client.clone(), &ns);

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
        while let Some(ev) = benches_stream.try_next().await? {
            match ev {
                kube::api::WatchEvent::Added(bench) => {
                    if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                        revision = rv;
                    }
                    let name = kube::api::Meta::name(&bench);
                    let mut state = ctx.state.lock().await;
                    state.active.insert(name.clone(), bench);
                    info!(%name, %revision, active = %state.active.len(), "Added");
                }
                kube::api::WatchEvent::Modified(bench) => {
                    if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                        revision = rv;
                    }
                    let name =  kube::api::Meta::name(&bench);
                    let mut state = ctx.state.lock().await;
                    state.active.insert(name.clone(), bench);
                    info!(%name, %revision, active = %state.active.len(), "Modified");
                }
                kube::api::WatchEvent::Deleted(bench) => {
                    if let Some(rv) = kube::api::Meta::resource_ver(&bench) {
                        revision = rv;
                    }
                    let name = kube::api::Meta::name(&bench);
                    let mut state = ctx.state.lock().await;
                    state.active.remove(&name);
                    info!(%name, %revision, active = %state.active.len(), "Deleted");
                }
                kube::api::WatchEvent::Bookmark(b) => {
                    revision = b.metadata.resource_version;
                }
                kube::api::WatchEvent::Error(error) => {
                    warn!(%error);
                    break;
                }
            }
        }
        info!("Stream completed");
    }
}
