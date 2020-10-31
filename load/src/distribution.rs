use indexmap::IndexMap;

/// Max: 1_000_000
#[derive(Copy, Clone, Debug, Hash, PartialOrd, PartialEq, Ord, Eq)]
pub struct Percentile(u32);

pub struct Distribution {
    // A sorted list of percentile-value pairs.
    percentiles: IndexMap<Percentile, u64>,
}

#[derive(Debug)]
pub enum InvalidDistribution {
    Unordered,
    InvalidValue,
    InvalidPercentile,
}

#[derive(Debug)]
pub struct InvalidPercentile(());

// === impl Distribution ===

impl Default for Distribution {
    fn default() -> Self {
        let mut percentiles = IndexMap::new();
        percentiles.entry(Percentile::MIN).or_insert(0);
        percentiles.entry(Percentile::MAX).or_insert(0);
        Self { percentiles }
    }
}

impl std::str::FromStr for Distribution {
    type Err = InvalidDistribution;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut pairs = Vec::new();
        for pv in s.split(',') {
            let mut pv = pv.splitn(2, '=');
            match (pv.next(), pv.next()) {
                (Some(p), Some(v)) => {
                    let p = p
                        .parse::<f32>()
                        .map_err(|_| InvalidDistribution::InvalidPercentile)?;
                    let v = v
                        .parse::<u64>()
                        .map_err(|_| InvalidDistribution::InvalidValue)?;
                    pairs.push((p, v));
                }
                _ => return Err(InvalidDistribution::InvalidPercentile)?,
            }
        }
        Self::build(pairs)
    }
}

impl Distribution {
    pub fn build<P>(pairs: impl IntoIterator<Item = (P, u64)>) -> Result<Self, InvalidDistribution>
    where
        P: std::convert::TryInto<Percentile>,
    {
        let mut percentiles = IndexMap::new();
        for (p, v) in pairs.into_iter() {
            let p = p
                .try_into()
                .map_err(|_| InvalidDistribution::InvalidPercentile)?;
            percentiles.insert(p, v);
        }

        // Ensure there is a minimum value in the distribution.
        percentiles.entry(Percentile::MIN).or_insert(0);
        percentiles.sort_keys();

        // Ensure all values are in ascending order.
        let mut base_v = 0u64;
        for v in percentiles.values() {
            if *v < base_v {
                return Err(InvalidDistribution::Unordered);
            }
            base_v = *v;
        }

        // Ensure there is a maximum value in the distribution.
        let max_v = base_v;
        percentiles.entry(Percentile::MAX).or_insert(max_v);

        Ok(Self { percentiles })
    }

    #[cfg(test)]
    pub fn min(&self) -> u64 {
        *self.percentiles.get(&Percentile::MIN).unwrap()
    }

    #[cfg(test)]
    pub fn max(&self) -> u64 {
        *self.percentiles.get(&Percentile::MAX).unwrap()
    }

    #[cfg(test)]
    pub fn try_get<P>(&self, p: P) -> Result<u64, P::Error>
    where
        P: std::convert::TryInto<Percentile>,
    {
        let p = p.try_into()?;
        Ok(self.get(p))
    }

    pub fn get(&self, Percentile(percentile): Percentile) -> u64 {
        let mut lower_p = 0u32;
        let mut lower_v = 0u64;
        for (Percentile(p), v) in self.percentiles.iter() {
            if *p == percentile {
                return *v;
            }

            if *p > percentile {
                let p_delta = *p as u64 - lower_p as u64;
                let added = if p_delta > 0 {
                    let v_delta = *v - lower_v;
                    let unit = v_delta as f64 / p_delta as f64;
                    let a = unit * ((percentile - lower_p) as u64) as f64;
                    a as u64
                } else {
                    0
                };
                return lower_v + added;
            }

            lower_p = *p;
            lower_v = *v;
        }

        unreachable!("percentile must exist in distribution");
    }
}

impl rand::distributions::Distribution<u64> for Distribution {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> u64 {
        self.get(rng.gen())
    }
}

// === impl Percentile ===

impl Percentile {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(100_0000);
    const FACTOR: u32 = 10000;
}

impl std::convert::TryFrom<f32> for Percentile {
    type Error = InvalidPercentile;

    fn try_from(v: f32) -> Result<Self, Self::Error> {
        if v < 0.0 || v > 100.0 {
            return Err(InvalidPercentile(()));
        }
        let adjusted = v * (Self::FACTOR as f32);

        Ok(Percentile(adjusted as u32))
    }
}

impl std::convert::TryFrom<u32> for Percentile {
    type Error = InvalidPercentile;

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        if v > 100 {
            return Err(InvalidPercentile(()));
        }
        Ok(Percentile(v * Self::FACTOR))
    }
}

impl rand::distributions::Distribution<Percentile> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Percentile {
        Percentile(rng.gen_range(0, 100_0000))
    }
}

// === impl InvalidDistribution ===

impl std::fmt::Display for InvalidDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unordered => write!(f, "Undordered distribution"),
            Self::InvalidPercentile => write!(f, "Invalid percentile"),
            Self::InvalidValue => write!(f, "Invalid value"),
        }
    }
}

impl std::error::Error for InvalidDistribution {}

impl std::fmt::Display for InvalidPercentile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid percentile")
    }
}

impl std::error::Error for InvalidPercentile {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn convert_percentiles() {
        assert_eq!(Percentile::try_from(0).unwrap(), Percentile::MIN);
        assert_eq!(Percentile::try_from(0.0).unwrap(), Percentile::MIN);
        assert_eq!(Percentile::try_from(50).unwrap(), Percentile(50_0000));
        assert_eq!(Percentile::try_from(50.0).unwrap(), Percentile(50_0000));
        assert_eq!(Percentile::try_from(75).unwrap(), Percentile(75_0000));
        assert_eq!(Percentile::try_from(75.0).unwrap(), Percentile(75_0000));
        assert_eq!(Percentile::try_from(99).unwrap(), Percentile(99_0000));
        assert_eq!(Percentile::try_from(99.0).unwrap(), Percentile(99_0000));
        assert_eq!(Percentile::try_from(99.99).unwrap(), Percentile(99_9900));
        assert_eq!(Percentile::try_from(99.99999).unwrap(), Percentile(99_9999));
        assert_eq!(Percentile::try_from(100).unwrap(), Percentile::MAX);
        assert_eq!(Percentile::try_from(100.0).unwrap(), Percentile::MAX);

        assert!(Percentile::try_from(-1.0).is_err());
        assert!(Percentile::try_from(101.0).is_err());
    }

    #[test]
    fn distributions() {
        let d = Distribution::default();
        assert_eq!(d.min(), 0);
        assert_eq!(d.try_get(50).unwrap(), 0);
        assert_eq!(d.max(), 0);

        let d = Distribution::build(vec![(0, 1000), (100, 2000)]).unwrap();
        assert_eq!(d.min(), 1000);
        assert_eq!(d.try_get(50).unwrap(), 1500);
        assert_eq!(d.max(), 2000);
    }
}
