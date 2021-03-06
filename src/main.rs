mod provider;
use provider::{accu, owm};

mod service;
use service::{Pos, Service, CITIES};

#[macro_use]
extern crate rocket;

use rocket::routes;
use rocket::State;
use rocket::{http::Status, response, Request, Response};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};

use std::env;

#[get("/")]
fn index() -> Status {
    Status::Ok
}

/// StatusCode is custom responder for pretty errors
#[derive(Debug, Deserialize, Serialize)]
struct StatusError {
    code: u16,
    error: String,
}

impl<'r> response::Responder<'r, 'static> for StatusError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        Response::build()
            .status(Status::from_code(self.code).expect("invalid status code"))
            .merge(Json(self).respond_to(request)?)
            .ok()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Current {
    pos: (f32, f32),
    temp: f32,
}

#[get("/current?<city>&<country>&<day>")]
async fn current(
    service: State<'_, Service>,
    country: String,
    city: String,
    day: Option<usize>,
) -> Result<Json<Current>, StatusError> {
    let day = day.unwrap_or(0);
    if day > 4 {
        return Err(StatusError {
            code: 400,
            error: "can't see further than 5 days".to_string(),
        });
    }

    let (pos, forecast) = fetch(&service, &country, &city).await?;

    Ok(Json(Current {
        pos: pos.as_lat_lon(),
        temp: forecast[day],
    }))
}

#[derive(Debug, Serialize, Deserialize)]
struct Forecast {
    pos: (f32, f32),
    forecast: [f32; 5],
}

#[get("/forecast?<city>&<country>")]
async fn forecast(
    service: State<'_, Service>,
    country: String,
    city: String,
) -> Result<Json<Forecast>, StatusError> {
    let (pos, forecast) = fetch(&service, &country, &city).await?;

    Ok(Json(Forecast {
        pos: pos.as_lat_lon(),
        forecast,
    }))
}

// Tiny service request helper
async fn fetch(
    service: &Service,
    country: &str,
    city: &str,
) -> Result<(Pos, [f32; 5]), StatusError> {
    let coordinates = CITIES
        .find(&country, &city.to_lowercase())
        .ok_or(StatusError {
            code: 404,
            error: "City not found".to_owned(),
        })?;

    match service.forecast(coordinates).await {
        Ok(resp) => Ok((coordinates, resp)),
        Err(err) => {
            Err(StatusError {
                code: 500,
                error: format!("{:#}", err), // Print full error chain
            })
        }
    }
}

#[rocket::main]
async fn main() -> Result<(), rocket::error::Error> {
    let token = env::var("OWM_TOKEN").expect("OWM_TOKEN env");
    let prov1 = owm::OWM::new(token);

    let token = env::var("ACCU_TOKEN").expect("ACCU_TOKEN env");
    let prov2 = accu::AccuWeather::new(token);

    let service = Service::new(Vec::from([prov1, prov2]));

    rocket::ignite()
        .mount("/", routes![index, current, forecast])
        .manage(service)
        .launch()
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider::fake::{erroneous, stub};
    use rocket::local::blocking::Client;
    use serde_json::Value;

    macro_rules! request {
        ($expected:ty, $url:expr) => {
            request!($expected, $url, Vec::from([stub(2.0), stub(4.0)]))
        };
        ($expected:ty, $url:expr, $providers:expr) => {{
            let service = Service::new(Vec::from($providers));

            let rocket = rocket::ignite()
                .manage(service)
                .mount("/", routes![index, forecast, current]);

            let client = Client::tracked(rocket).expect("initialize rocket");

            let response = client.get($url).dispatch();
            let status = response.status();

            let body = response.into_string().unwrap_or_else(|| "{}".to_owned());
            let reply: $expected = serde_json::from_str(&body).expect("parse reply");

            (status, reply)
        }};
    }

    #[test]
    fn index_endpoint() {
        let (status, _) = request!(Value, "/");
        assert_eq!(status, Status::Ok)
    }

    #[test]
    fn forecast_endpoint() {
        let (status, response) = request!(Forecast, "/forecast?country=US&city=Chicago");

        assert_eq!(status, Status::Ok);
        assert_eq!(response.pos, (41.85003, -87.65005));
        assert_eq!(response.forecast, [3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn forecast_unknown_city() {
        let (status, response) = request!(StatusError, "/forecast?country=US&city=Sanity");

        assert_eq!(status, Status::NotFound);
        assert_eq!(response.code, 404);
        assert_eq!(response.error, "City not found");
    }

    #[test]
    fn current_endpoint() {
        let (status, response) = request!(Current, "/current?country=RU&city=Moscow");

        assert_eq!(status, Status::Ok);
        assert_eq!(response.pos, (55.75222, 37.61556));
        assert_eq!(response.temp, 3.0);

        let (status, response) = request!(Current, "/current?country=RU&city=Moscow&day=1");
        assert_eq!(status, Status::Ok);
        assert_eq!(response.temp, 4.0);
    }

    #[test]
    fn current_sixth_day() {
        let (status, response) = request!(StatusError, "/current?country=RU&city=Moscow&day=5");
        assert_eq!(status, Status::BadRequest);
        assert_eq!(response.error, "can\'t see further than 5 days");
    }

    #[test]
    fn error_propagation() {
        let (status, response) = request!(
            StatusError,
            "/current?country=DE&city=Berlin",
            [erroneous("something bad happened")]
        );

        assert_eq!(status, Status::InternalServerError);
        assert_eq!(
            response.error,
            "error while fetching forecast: something bad happened"
        );
    }
}
