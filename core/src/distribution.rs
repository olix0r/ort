use indexmap::IndexMap;
use std::str::FromStr;

/// Max: 1_000_000
#[derive(Copy, Clone, Debug, Hash, PartialOrd, PartialEq, Ord, Eq)]
pub struct Percentile(u32);

pub struct Distribution<T = u64> {
    // A sorted list of percentile-value pairs.
    percentiles: IndexMap<Percentile, u64>,
    _marker: std::marker::PhantomData<T>,
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

impl<T> Clone for Distribution<T> {
    fn clone(&self) -> Self {
        Self {
            percentiles: self.percentiles.clone(),
            _marker: self._marker,
        }
    }
}

impl<T> std::fmt::Debug for Distribution<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Distribution")
            .field("percentiles", &self.percentiles)
            .finish()
    }
}

impl<T: Default + Into<u64>> Default for Distribution<T> {
    fn default() -> Self {
        let mut percentiles = IndexMap::new();
        let v = T::default().into();
        percentiles.entry(Percentile::MIN).or_insert(v);
        percentiles.entry(Percentile::MAX).or_insert(v);
        Self {
            percentiles,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: FromStr + Default + Into<u64>> FromStr for Distribution<T> {
    type Err = InvalidDistribution;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pvs = s.split(',').collect::<Vec<_>>();
        if pvs.len() == 1 {
            // If there is only a single item, it may not necessarily have a percentile.
            let pv = match pvs[0].splitn(2, '=').collect::<Vec<_>>().as_slice() {
                [v] => {
                    let v = v
                        .parse::<T>()
                        .map_err(|_| InvalidDistribution::InvalidValue)?;
                    (0f32, v)
                }
                [p, v] => {
                    let p = p
                        .parse::<f32>()
                        .map_err(|_| InvalidDistribution::InvalidPercentile)?;
                    let v = v
                        .parse::<T>()
                        .map_err(|_| InvalidDistribution::InvalidValue)?;
                    (p, v)
                }
                _ => return Err(InvalidDistribution::InvalidPercentile)?,
            };
            Self::build(Some(pv))
        } else {
            let mut pairs = Vec::new();
            for pv in pvs {
                let mut pv = pv.splitn(2, '=');
                match (pv.next(), pv.next()) {
                    (Some(p), Some(v)) => {
                        let p = p
                            .parse::<f32>()
                            .map_err(|_| InvalidDistribution::InvalidPercentile)?;
                        let v = v
                            .parse::<T>()
                            .map_err(|_| InvalidDistribution::InvalidValue)?;
                        pairs.push((p, v));
                    }
                    _ => return Err(InvalidDistribution::InvalidPercentile)?,
                }
            }
            Self::build(pairs)
        }
    }
}

impl<T: Default + Into<u64>> Distribution<T> {
    pub fn build<P>(pairs: impl IntoIterator<Item = (P, T)>) -> Result<Self, InvalidDistribution>
    where
        P: std::convert::TryInto<Percentile>,
    {
        let mut percentiles = IndexMap::new();
        for (p, v) in pairs.into_iter() {
            let p = p
                .try_into()
                .map_err(|_| InvalidDistribution::InvalidPercentile)?;
            percentiles.insert(p, v.into());
        }

        // Ensure there is a minimum value in the distribution.
        percentiles
            .entry(Percentile::MIN)
            .or_insert(T::default().into());
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

        Ok(Self {
            percentiles,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<T: From<u64>> Distribution<T> {
    #[cfg(test)]
    pub fn min(&self) -> T {
        let v = self.percentiles.get(&Percentile::MIN).unwrap();
        (*v).into()
    }

    #[cfg(test)]
    pub fn max(&self) -> T {
        let v = self.percentiles.get(&Percentile::MAX).unwrap();
        (*v).into()
    }

    #[cfg(test)]
    pub fn try_get<P>(&self, p: P) -> Result<T, P::Error>
    where
        P: std::convert::TryInto<Percentile>,
    {
        let p = p.try_into()?;
        Ok(self.get(p))
    }

    pub fn get(&self, Percentile(percentile): Percentile) -> T {
        let mut lower_p = 0u32;
        let mut lower_v = 0u64;
        for (Percentile(p), v) in self.percentiles.iter() {
            if *p == percentile {
                return (*v).into();
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
                return (lower_v + added).into();
            }

            lower_p = *p;
            lower_v = *v;
        }

        unreachable!("percentile must exist in distribution");
    }
}

impl<T: From<u64>> rand::distributions::Distribution<T> for Distribution<T> {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> T {
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
        Percentile(rng.gen_range(0..=100_0000))
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
        let d = Distribution::<u64>::default();
        assert_eq!(d.min(), 0);
        assert_eq!(d.try_get(50).unwrap(), 0);
        assert_eq!(d.max(), 0);

        let d = Distribution::build(vec![(0, 1000u64), (100, 2000)]).unwrap();
        assert_eq!(d.min(), 1000);
        assert_eq!(d.try_get(50).unwrap(), 1500);
        assert_eq!(d.max(), 2000);
    }

    #[test]
    fn parse() {
        let d = "123".parse::<Distribution<u64>>().unwrap();
        assert_eq!(d.min(), 123);
        assert_eq!(d.try_get(50).unwrap(), 123);
        assert_eq!(d.max(), 123);

        let d = "50=123".parse::<Distribution<u64>>().unwrap();
        assert_eq!(d.min(), 0);
        assert_eq!(d.try_get(50).unwrap(), 123);
        assert_eq!(d.max(), 123);

        let d = "0=1,50=123,100=234".parse::<Distribution<u64>>().unwrap();
        assert_eq!(d.min(), 1);
        assert_eq!(d.try_get(50).unwrap(), 123);
        assert_eq!(d.max(), 234);

        #[derive(Debug, Default, PartialEq, Eq)]
        struct Dingus(u64);
        impl From<u64> for Dingus {
            fn from(n: u64) -> Self {
                Self(n)
            }
        }
        impl Into<u64> for Dingus {
            fn into(self) -> u64 {
                self.0
            }
        }
        impl std::str::FromStr for Dingus {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, ()> {
                match s {
                    "A" => Ok(Self(10)),
                    "B" => Ok(Self(20)),
                    "C" => Ok(Self(30)),
                    "D" => Ok(Self(40)),
                    _ => Err(()),
                }
            }
        }

        let d = "0=A,50=B,90=C,100=D"
            .parse::<Distribution<Dingus>>()
            .unwrap();
        assert_eq!(d.min(), Dingus(10));
        assert_eq!(d.try_get(50).unwrap(), Dingus(20));
        assert_eq!(d.try_get(90).unwrap(), Dingus(30));
        assert_eq!(d.try_get(95).unwrap(), Dingus(35));
        assert_eq!(d.max(), Dingus(40));
    }
}
