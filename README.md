# Weather

> Write a Rust RESTful web service. The service must return the weather forecast (temperature) in a given city:
> - for a given day (current or next, no need to work with historical data)
> For the next week (a collection of 5 days)
> Select a pair of third-party web services (with an open API) as the data source. You need to calculate the average of the data from both of them.
> In implementation, when selecting one or the other, you should be guided by what you would prefer to use in the actual application.
> It's not necessary but it will be a plus if you:
> - cover the code with unit and functional tests
> Send informative errors to API requests
> Dockerize service


### Configure and run
```sh
export OWM_TOKEN="blah" # https://home.openweathermap.org/
export ACCU_TOKEN="blah" # https://developer.accuweather.com
cargo test
cargo run

# using
curl "localhost:8000/forecast?country=RU&city=Moscow" # 5-day forecast
curl "localhost:8000/current?country=RU&city=Saint%20Petersburg" # For today
curl "localhost:8000/current?country=US&city=Chicago&day=1" # For tomorrow

# Docker
sudo docker build -t weather .
sudo docker run -e OWM_TOKEN="blah" -e ACCU_TOKEN="blah" -p "8000:8000" weather
```

### Technical points

**Rocket** is an ergonomic web framework with zero boilerplate. The latest dev version with async I/O support is used.

**Anyhow** - Allows you to add context when handling errors in the application.

**Reqwest** - Almost like a standard http client for Rust.

**Reqwest** - Minimal number of dependencies. Used the capabilities of the standard library as much as possible.

* City coordinates are stored in memory because different sources may not support a search by city name.

* To reduce the API limits a simple caching of the results by the day of the request was implemented.

* New forecast sources are easy to add through the `Provider` interface. The service supports calculation of average temperature values for any number of providers.


### License
WTFPL

