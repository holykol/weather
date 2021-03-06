use crate::provider::DynProvider;
use serde::Deserialize;

use futures::future;
use lazy_static::lazy_static;

use std::collections::HashMap;
use std::sync::Arc;
use std::{borrow::Cow, cmp, hash};
use tokio::sync::RwLock;

use chrono::offset::Local;
use chrono::Date;

use anyhow::{Context, Result};

/// City location latitude and longitude pair with custom guarantees
#[derive(Debug, Clone, Copy)]
pub struct Pos(f32, f32);

// Since all cities have fixed postitions and we guarantee they can't be tampered
// with (by not marking fields public) the following trait implementations are logically correct
impl hash::Hash for Pos {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.0.to_ne_bytes());
        state.write(&self.1.to_ne_bytes());
    }
}

impl cmp::PartialEq for Pos {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() <= f32::EPSILON && (self.1 - other.1).abs() <= f32::EPSILON
    }
}

impl cmp::Eq for Pos {}

impl Pos {
    pub fn as_lat_lon(&self) -> (f32, f32) {
        (self.0, self.1)
    }
}

#[derive(Debug, Deserialize)]
struct City<'a> {
    country: &'a str,
    name: &'a str,
    lat: f32,
    lng: f32,
}

// Local city coordinates mapping to avoid relying on provider search feature
type Cities<'a> = HashMap<Cow<'a, str>, HashMap<Cow<'a, str>, Pos>>;

lazy_static! {
    pub static ref CITIES: Cities<'static> = {
        let positions: Vec<City> =
            serde_json::from_str(include_str!("../cities.json")).expect("parse city database");

        let mut cities: Cities = HashMap::new();

        for city in positions {
            let country = cities.entry(city.country.into()).or_default();
            let name = city.name.to_lowercase();
            country.insert(name.into(), Pos(city.lat, city.lng));
        }

        cities
    };
}

impl CITIES {
    pub fn find(&self, country: &str, city: &str) -> Option<Pos> {
        let country = self.get(country)?;
        country.get(city).copied()
    }
}

pub struct Service {
    providers: Vec<DynProvider>,
    cache: Arc<RwLock<HashMap<Pos, CacheEntry>>>,
}

struct CacheEntry {
    date: Date<Local>,
    forecast: [f32; 5],
}

/// Serivce is responsible for computing aggregates and caching results
impl Service {
    pub fn new(providers: Vec<DynProvider>) -> Self {
        if providers.len() == 0 {
            panic!("tried to initialize weather service with zero providers")
        }

        Self {
            providers,
            cache: Default::default(),
        }
    }

    pub async fn forecast(&self, pos: Pos) -> Result<[f32; 5]> {
        let rcache = self.cache.read().await;

        if let Some(entry) = rcache.get(&pos) {
            if entry.date == Local::today() {
                return Ok(entry.forecast);
            }
        }

        // Slow path
        drop(rcache);
        let result = self.fetch_forecast(pos).await?;

        let entry = CacheEntry {
            date: Local::today(),
            forecast: result,
        };
        self.cache.write().await.insert(pos, entry);

        Ok(result)
    }

    async fn fetch_forecast(&self, pos: Pos) -> Result<[f32; 5]> {
        let mut futures = Vec::with_capacity(self.providers.len());

        // Fetch data in parallel
        for provider in &self.providers {
            futures.push(provider.fetch(pos));
        }

        let mut avg = [0.0; 5];

        for result in future::join_all(futures).await {
            let result = result.context("error while fetching forecast")?;

            for (i, t) in avg.iter_mut().enumerate() {
                *t += result[i];
            }
        }

        for t in &mut avg {
            *t /= self.providers.len() as f32;
        }

        Ok(avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::fake::{erroneous, stub};
    use chrono::Duration;

    #[rocket::async_test]
    async fn service() {
        let service = Service::new(Vec::from([stub(-8.0), stub(6.0)]));
        let pos = CITIES.find("US", "chicago").unwrap();

        // Test old caches are invalidated
        let entry = CacheEntry {
            date: Local::today() - Duration::days(2),
            forecast: [0.0; 5],
        };

        service.cache.write().await.insert(pos, entry);

        let avg = service.forecast(pos).await.unwrap();

        assert_eq!(
            avg.iter().sum::<f32>(),
            5.0,
            "average temperature did not equal expected"
        );
    }

    #[rocket::async_test]
    async fn service_caching() {
        let service = Service::new(Vec::from([erroneous("shouldn't be called")]));
        let pos = CITIES.find("US", "chicago").unwrap();

        let entry = CacheEntry {
            date: Local::today(),
            forecast: [5.0; 5],
        };

        service.cache.write().await.insert(pos, entry);

        let sum = service.forecast(pos).await.unwrap().iter().sum::<f32>();

        assert_eq!(sum, 25.0, "did not use cache");
    }
}
