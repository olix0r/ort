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
pub struct Status {
    is_bad: bool,
}

struct Ctx {
    client: kube::Client,
    state: Mutex<State>,
}

struct State {
    active: IndexMap<String, Bench>,
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {
    let ns = std::env::var("NAMESPACE")
        .ok()
        .filter(|ns| ns.is_empty())
        .unwrap_or_else(|| "default".to_string());

    let client = kube::Client::try_default().await?;

    let benches_api = kube::Api::<Bench>::namespaced(client.clone(), &ns);
    let _svc_api = kube::Api::<Service>::namespaced(client.clone(), &ns);
    let _pods_api = kube::Api::<Pod>::namespaced(client.clone(), &ns);

    let ctx = Arc::new(Ctx {
        client,
        state: Mutex::new(State {
            active: IndexMap::default(),
        }),
    });

    let mut revision = "0".to_string();

    loop {
        let benches_params = kube::api::ListParams::default();
        let mut benches_stream = benches_api.watch(&benches_params, &revision).await?.boxed();
        while let Some(ev) = benches_stream.try_next().await? {
            match ev {
                kube::api::WatchEvent::Added(b) => {
                    if let Some(rv) = kube::api::Meta::resource_ver(&b) {
                        revision = rv;
                    }
                    let name = kube::api::Meta::name(&b);
                    ctx.state.lock().await.active.insert(name, b);
                    let _ = ctx.client;
                }
                kube::api::WatchEvent::Modified(b) => unimplemented!("Modified {:?}", b),
                kube::api::WatchEvent::Deleted(b) => unimplemented!("Deleted {:?}", b),
                kube::api::WatchEvent::Bookmark(_) => unimplemented!("Bookmark"),
                kube::api::WatchEvent::Error(e) => {
                    eprintln!("{}", e);
                }
            }
        }
    }
}

// === Error ===

#[derive(Debug)]
pub enum Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Error {}
