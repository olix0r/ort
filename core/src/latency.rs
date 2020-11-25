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

impl rand::distributions::Distribution<Duration> for Distribution {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Duration {
        let Latency { millis } = self.get(rng.gen());
        Duration::from_millis(millis)
    }
}

// === parse_duration ===

pub fn parse_duration(s: &str) -> Result<Duration, InvalidDuration> {
    use regex::Regex;

    let re = Regex::new(r"^\s*(\d+)(ms|s)?\s*$").expect("duration regex");
    let cap = re.captures(s).ok_or(InvalidDuration(()))?;
    let magnitude = cap[1].parse().map_err(|_| InvalidDuration(()))?;
    match cap.get(2).map(|m| m.as_str()) {
        None if magnitude == 0 => Ok(Duration::from_millis(0)),
        Some("ms") => Ok(Duration::from_millis(magnitude)),
        Some("s") => Ok(Duration::from_secs(magnitude)),
        _ => Err(InvalidDuration(())),
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InvalidDuration(());

impl std::fmt::Display for InvalidDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid duration")
    }
}

impl std::error::Error for InvalidDuration {}
