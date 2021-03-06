use super::Pos;
use anyhow::{anyhow, Context, Result};
use reqwest;
use serde::Deserialize;
use std::time::Duration;

pub type DynProvider = Box<dyn Provider + Send + Sync>;

/// Provider is responsible for fetching weekly weather forecast from its source
#[rocket::async_trait]
pub trait Provider {
    async fn fetch(&self, pos: Pos) -> Result<[f32; 5]>;
}

// https://openweathermap.org/api/one-call-api
pub mod owm {
    use super::*;
    use reqwest::StatusCode;

    pub struct OWM {
        token: String,
    }

    impl OWM {
        // Should probably return Self to allow futher customization
        // in the future
        pub fn new(token: String) -> DynProvider {
            Box::new(OWM { token })
        }
    }

    #[derive(Deserialize)]
    struct Response {
        daily: [Daily; 8],
    }

    #[derive(Deserialize)]
    struct Daily {
        temp: Temp,
    }

    #[derive(Deserialize)]
    struct Temp {
        day: f32,
        night: f32,
    }

    #[derive(Deserialize)]
    struct Error {
        message: String,
    }

    #[rocket::async_trait]
    impl Provider for OWM {
        async fn fetch(&self, pos: Pos) -> Result<[f32; 5]> {
            let pos = pos.as_lat_lon();

            let request = reqwest::Client::new()
                .get("https://api.openweathermap.org/data/2.5/onecall")
                .timeout(Duration::from_secs(10))
                .query(&[("lat", pos.0), ("lon", pos.1)])
                .query(&[
                    ("exclude", "current,minutely,hourly,alerts"),
                    ("units", "metric"),
                    ("appid", &self.token),
                ]);

            let response = request.send().await.context("error requesting provider")?;

            if response.status() != StatusCode::OK {
                let error = response.json::<Error>().await?;
                return Err(anyhow!("external provider error: {}", error.message));
            }

            let response = response
                .json::<Response>()
                .await
                .context("error parsing response")?;

            let mut result = [0.0; 5];

            for (i, day) in response.daily.iter().take(5).enumerate() {
                result[i] = (day.temp.day + day.temp.night) / 2.0;
            }

            Ok(result)
        }
    }
}

// https://developer.accuweather.com/
pub mod accu {
    use super::*;

    pub struct AccuWeather {
        token: String,
    }

    impl AccuWeather {
        pub fn new(token: String) -> DynProvider {
            Box::new(AccuWeather { token })
        }

        // This api requires getting ID of location first
        async fn search(&self, pos: Pos) -> Result<String> {
            let pos = pos.as_lat_lon();

            let search_response = reqwest::Client::new()
                .get("https://dataservice.accuweather.com/locations/v1/cities/geoposition/search")
                .timeout(Duration::from_secs(10))
                .query(&[
                    ("apikey", &self.token),
                    ("q", &format!("{},{}", pos.0, pos.1)),
                ])
                .send()
                .await?
                .json::<SearchResponse>()
                .await?;

            Ok(search_response.key)
        }
    }

    #[derive(Deserialize)]
    pub struct SearchResponse {
        #[serde(rename = "Key")]
        key: String,
    }

    #[derive(Deserialize)]
    pub struct ForecastResponse {
        #[serde(rename = "DailyForecasts")]
        daily_forecasts: [DailyForecast; 5],
    }

    #[derive(Deserialize)]
    pub struct DailyForecast {
        #[serde(rename = "Temperature")]
        temperature: Temperature,
    }

    #[derive(Deserialize)]
    pub struct Temperature {
        #[serde(rename = "Minimum")]
        minimum: Imum,
        #[serde(rename = "Maximum")]
        maximum: Imum,
    }

    #[derive(Deserialize)]
    pub struct Imum {
        #[serde(rename = "Value")]
        value: f32,
    }

    #[rocket::async_trait]
    impl Provider for AccuWeather {
        async fn fetch(&self, pos: Pos) -> Result<[f32; 5]> {
            let key = self.search(pos).await?;

            let url = format!(
                "http://dataservice.accuweather.com/forecasts/v1/daily/5day/{}",
                key
            );

            let response = reqwest::Client::new()
                .get(&url)
                .timeout(Duration::from_secs(10))
                .query(&[("metric", "true"), ("apikey", &self.token)])
                .send()
                .await
                .context("error fetching data")?;

            let response = response
                .json::<ForecastResponse>()
                .await
                .context("error parsing response")?;

            let mut result = [0.0; 5];

            for (i, day) in response.daily_forecasts.iter().enumerate() {
                let min = day.temperature.minimum.value;
                let max = day.temperature.maximum.value;
                result[i] = (min + max) / 2.0;
            }

            Ok(result)
        }
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use anyhow::anyhow;

    struct FakeProvider(Result<f32>);

    #[rocket::async_trait]
    impl Provider for FakeProvider {
        async fn fetch(&self, _pos: Pos) -> Result<[f32; 5]> {
            match self.0 {
                Ok(n) => {
                    let mut result = [n; 5];
                    for i in (0..5) {
                        result[i] += i as f32
                    }

                    anyhow::Result::Ok(result)
                }
                Err(ref e) => Err(anyhow!(e.to_string())),
            }
        }
    }

    pub fn stub(n: f32) -> DynProvider {
        Box::new(FakeProvider(Ok(n)))
    }

    pub fn erroneous(e: &str) -> DynProvider {
        Box::new(FakeProvider(Err(anyhow!(e.to_owned()))))
    }
}
