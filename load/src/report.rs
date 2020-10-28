use hdrhistogram::sync::SyncHistogram;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Report {
    duration_histogram: Histogram,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Histogram {
    count: u64,
    min: f64,
    max: f64,
    sum: f64,
    avg: f64,
    std_dev: f64,
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

impl From<&'_ SyncHistogram<u64>> for Report {
    fn from(h: &SyncHistogram<u64>) -> Self {
        Self {
            duration_histogram: Histogram::from(h),
        }
    }
}

impl From<&'_ SyncHistogram<u64>> for Histogram {
    fn from(h: &SyncHistogram<u64>) -> Self {
        // let buckets = h
        //     .iter_recorded()
        //     .map(|v| Bucket {
        //         start: 0,
        //         end: 0,
        //         percent: v.percentile(),
        //         count: 0,
        //     })
        //     .collect::<Vec<_>>();

        const PERCENTILES: [f64; 5] = [50.0, 75.0, 90.0, 99.0, 99.9];
        let mut percentiles = Vec::with_capacity(PERCENTILES.len());
        for p in &PERCENTILES {
            percentiles.push(Percentile {
                percentile: *p,
                value: h.value_at_percentile(*p) as f64,
            })
        }

        Self {
            count: h.len(),
            avg: h.mean(),
            min: h.min() as f64,
            max: h.max() as f64,
            std_dev: h.stdev(),
            percentiles,
            ..Histogram::default()
        }
    }
}
