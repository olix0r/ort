use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube;
use kube::CustomResource;
use serde::{Deserialize, Serialize};

#[kube(
    group = "ortiofay.olix0r.net",
    version = "v1alpha1",
    kind = "Bench",
    namespaced,
    status = "Status"
)]
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    load: Load,
    server: Server,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    metadata: ObjectMeta,
    spec: LoadSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSpec {
    requests_per_second: usize,
    total_requests: usize,
    concurrency: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    metadata: ObjectMeta,
    spec: ServerSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSpec {
    replicas: Option<u32>,
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
