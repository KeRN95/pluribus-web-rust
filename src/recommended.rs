//! "Të rekomanduara për ty": the welcome page's listings for a visitor who has shown what they
//! look for, in place of the newest (web/browse.rs, home), as the user asked on 2026-10-11. The
//! site keeps no history of a visitor's: the seeds are what there is, the listings they saw
//! lately (SEEN_COOKIE, which their browser keeps) and, signed in, those they keep and those they
//! wrote to a seller about (db::seed_listings), each weighed (Seeds). The listings like them are
//! ranked for all of them at once (db::recommended_ids), and a visitor's ranking is kept for a
//! while (Recommended), so that the pages after the first, whose addresses name the seeds, go on
//! in the same order.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sqlx::PgPool;
use uuid::Uuid;

use crate::db;

/// The most seeds a visitor's recommendations grow from, the strongest: as many as the listings
/// seen lately (SEEN_MOST, 12), those kept (12) and those written about (6) can be.
pub const SEEDS_MOST: usize = 30;

/// A seed's weight, in tenths: the listing seen last 10, each seen before it .85 of the one after
/// (the fifth 5, the twelfth 2); one kept 15; one written to the seller about 20, writing being
/// the nearest to buying. A listing in more than one adds them up.
const SEEN: f64 = 10.0;
const SEEN_FADES: f64 = 0.85;
const KEPT: f64 = 15.0;
const WRITTEN: f64 = 20.0;

/// How many listings are recommended for each seed, a page's worth (db::PAGE_SIZE), and at most:
/// five pages. The newest follow them.
const PER_SEED: usize = 24;
const MOST: usize = 120;

/// The fewest the welcome page shows under "Të rekomanduara për ty": with fewer like what the
/// visitor looked at, it shows the newest, as for one who looked at none.
pub const FEWEST: usize = 4;

/// The listings a visitor has shown an interest in, by number, each with its weight in tenths,
/// the strongest first (then the lowest number): the seeds of their recommendations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Seeds(Vec<(i64, u8)>);

impl Seeds {
    /// The seeds of the listings seen lately (`seen`, the latest first), those kept and those
    /// written about, SEEDS_MOST at most.
    pub fn from_signals(seen: &[i64], kept: &[i64], written: &[i64]) -> Self {
        let seen = seen.iter().enumerate().map(|(i, number)| (*number, SEEN * SEEN_FADES.powi(i as i32)));
        let kept = kept.iter().map(|number| (*number, KEPT));
        let written = written.iter().map(|number| (*number, WRITTEN));
        let mut weights: Vec<(i64, f64)> = Vec::new();
        for (number, weight) in seen.chain(kept).chain(written) {
            match weights.iter_mut().find(|(seed, _)| *seed == number) {
                Some((_, sum)) => *sum += weight,
                None => weights.push((number, weight)),
            }
        }
        let tenths = |weight: f64| weight.round().clamp(1.0, 255.0) as u8;
        Self::sorted(weights.into_iter().map(|(number, weight)| (number, tenths(weight))).collect())
    }

    /// Seeds as an address names them (`param`): "1002x24.1005x20", each listing's number and its
    /// weight in tenths. None for anything else: no seed, a number twice, more than SEEDS_MOST.
    pub fn parse(value: &str) -> Option<Self> {
        if value.split('.').count() > SEEDS_MOST {
            return None;
        }
        let mut seeds: Vec<(i64, u8)> = Vec::new();
        for part in value.split('.') {
            let (number, weight) = part.split_once('x')?;
            let number = number.parse::<i64>().ok().filter(|number| *number > 0)?;
            let weight = weight.parse::<u8>().ok().filter(|weight| *weight > 0)?;
            if seeds.iter().any(|(seed, _)| *seed == number) {
                return None;
            }
            seeds.push((number, weight));
        }
        Some(Self::sorted(seeds))
    }

    fn sorted(mut seeds: Vec<(i64, u8)>) -> Self {
        seeds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        seeds.truncate(SEEDS_MOST);
        Self(seeds)
    }

    /// The seeds as the addresses of the pages after the first name them: "1002x24.1005x20".
    pub fn param(&self) -> String {
        self.0.iter().map(|(number, weight)| format!("{number}x{weight}")).collect::<Vec<_>>().join(".")
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each seed's number and weight, for db::recommended_ids: 1.5 for one kept.
    pub fn weighed(&self) -> Vec<(i64, f64)> {
        self.0.iter().map(|(number, weight)| (*number, f64::from(*weight) / 10.0)).collect()
    }

    /// How many listings are recommended at most: PER_SEED for each seed, MOST in all.
    pub fn limit(&self) -> i64 {
        (PER_SEED * self.0.len()).min(MOST) as i64
    }
}

/// How long a visitor's ranking is kept: the pages after the first go on in its order meanwhile,
/// and it is worked out once. A listing put up meanwhile waits for the next.
const KEEP: Duration = Duration::from_secs(15 * 60);

/// How many visitors' rankings are kept at most (MOST ids each, some 2 KB): past it, those kept
/// longer than KEEP go, then the oldest.
const KEPT_MOST: usize = 4096;

/// Each visitor's recommended listings, as db::recommended_ids ranked them, by a hash of their
/// seeds, who they are signed in as and the accounts they blocked, with when they were ranked.
#[derive(Clone)]
pub struct Recommended(Arc<Inner>);

struct Inner {
    /// Zero: none are kept, each ranked as it is asked for (the tests).
    keep: Duration,
    ranked: Mutex<Rankings>,
}

/// The rankings kept, by their key (Recommended::ranked), each with when it was ranked.
type Rankings = HashMap<u64, (Instant, Arc<[Uuid]>)>;

impl Recommended {
    /// Each kept for KEEP.
    pub fn kept() -> Self {
        Self::keeping(KEEP)
    }

    /// None kept, for the tests.
    pub fn immediate() -> Self {
        Self::keeping(Duration::ZERO)
    }

    fn keeping(keep: Duration) -> Self {
        Self(Arc::new(Inner { keep, ranked: Mutex::default() }))
    }

    /// The ids of the listings recommended for these seeds, the most alike first: as kept, or
    /// ranked again (db::recommended_ids) once KEEP has gone. None for no seeds.
    pub async fn ranked(
        &self,
        db: &PgPool,
        seeds: &Seeds,
        viewer: Option<Uuid>,
        hidden: &[Uuid],
    ) -> sqlx::Result<Arc<[Uuid]>> {
        if seeds.is_empty() {
            return Ok(Arc::from(Vec::new()));
        }
        let mut hidden = hidden.to_vec();
        hidden.sort_unstable();
        let mut hasher = DefaultHasher::new();
        (seeds, viewer, &hidden).hash(&mut hasher);
        let key = hasher.finish();
        let keep = self.0.keep;
        if let Some((at, ids)) = self.0.ranked.lock().expect("the rankings' lock").get(&key)
            && at.elapsed() < keep
        {
            return Ok(ids.clone());
        }
        let ids: Arc<[Uuid]> =
            db::recommended_ids(db, &seeds.weighed(), viewer, &hidden, seeds.limit()).await?.into();
        if !keep.is_zero() {
            let mut ranked = self.0.ranked.lock().expect("the rankings' lock");
            if ranked.len() >= KEPT_MOST {
                ranked.retain(|_, (at, _)| at.elapsed() < keep);
            }
            if ranked.len() >= KEPT_MOST
                && let Some(oldest) = ranked.iter().min_by_key(|(_, (at, _))| *at).map(|(key, _)| *key)
            {
                ranked.remove(&oldest);
            }
            ranked.insert(key, (Instant::now(), ids.clone()));
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_weigh_what_the_visitor_did() {
        // Seen lately the Golf, then the Mercedes, then the flat; the Mercedes kept; the phone
        // written about: the Mercedes, seen and kept, weighs the most, then the phone, the Golf,
        // the flat.
        let seeds = Seeds::from_signals(&[1001, 1002, 1003], &[1002], &[1005]);
        assert_eq!(seeds.param(), "1002x24.1005x20.1001x10.1003x7");
        let numbers: Vec<i64> = seeds.weighed().iter().map(|(number, _)| *number).collect();
        assert_eq!(numbers, [1002, 1005, 1001, 1003]);
        // A page for each seed.
        assert_eq!(seeds.limit(), 96);
        assert_eq!(Seeds::parse(&seeds.param()), Some(seeds));
        // Five pages at most.
        let many: Vec<i64> = (1..=12).collect();
        assert_eq!(Seeds::from_signals(&many, &[], &[]).limit(), 120);
        assert!(Seeds::from_signals(&[], &[], &[]).is_empty());
    }

    #[test]
    fn an_address_names_seeds_the_strongest_first() {
        let parsed = Seeds::parse("1005x20.1002x24").map(|seeds| seeds.param());
        assert_eq!(parsed.as_deref(), Some("1002x24.1005x20"));
        let wrong = ["", "1001", "1001x", "x10", "1001x0", "1001x256", "0x10", "-5x10", "1001x10.", "1001x10.1001x5", "1001x10x2"];
        for wrong in wrong {
            assert_eq!(Seeds::parse(wrong), None, "{wrong}");
        }
        let too_many = (1..=31).map(|number| format!("{number}x10")).collect::<Vec<_>>().join(".");
        assert_eq!(Seeds::parse(&too_many), None);
    }
}
