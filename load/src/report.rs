use hdrhistogram::sync::SyncHistogram;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Report {
    duration_histogram: ReportHistogram,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ReportHistogram {
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
#[serde(rename_all = "PascalCase")]
struct Bucket {
    start: f64,
    end: f64,
    percent: f64,
    count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Percentile {
    percentile: f64,
    value: f64,
}

impl From<Arc<SyncHistogram<u64>>> for Report {
    fn from(_histo: Arc<SyncHistogram<u64>>) -> Self {
        // let buckets = self
        //     .histogram
        //     .iter_recorded()
        //     .map(|v| Bucket {
        //         start: 0,
        //         end: 0,
        //         percent: v.percentile(),
        //         count: 0,
        //     })
        //     .collect::<Vec<_>>();
        Self::default()
    }
}
