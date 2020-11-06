use std::time::Duration;

pub type Distribution = crate::Distribution<Latency>;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Latency {
    millis: u64,
}

impl From<u64> for Latency {
    fn from(millis: u64) -> Self {
        Self { millis }
    }
}

impl Into<u64> for Latency {
    fn into(self) -> u64 {
        self.millis
    }
}

impl std::str::FromStr for Latency {
    type Err = crate::InvalidDuration;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let d = crate::parse_duration(s)?;
        Ok((d.as_millis() as u64).into())
    }
}

impl rand::distributions::Distribution<::prost_types::Duration> for Distribution {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> ::prost_types::Duration {
        let Latency { millis } = self.get(rng.gen());
        Duration::from_millis(millis).into()
    }
}
