use chrono::Local;
use clap::{Parser, Subcommand};
use dht22_pi::{read, Reading, ReadingError};
use env_logger::Builder;
use futures;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{
    fs, io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{self, time};

const DEFAULT_REFRESH_SECS: i32 = 900; // default is 15 minutes

#[clap(name = "RPi Temperature Monitoring Service", author = "Laurynas Keturakis")]
#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the service that will ping sensors every set number of minutes (default: 15m)
    /// and send the data to your Grafana Cloud graphite instance
    #[command(name = "serve")]
    Serve(ServeArguments),

    /// Check the readings of a sensor once (useful for debugging)
    #[command(name = "check")]
    Check(CheckArguments),
}

#[derive(Parser)]
struct ServeArguments {
    /// Optional: test flag if provided will simply run the program
    /// without sending the data to Grafana
    #[arg(long, short)]
    debug: bool,

    /// Refresh time - how often should the temperature be sampled and supplied to Grafana Cloud (Graphite)
    /// Provide a number in seconds
    #[arg(long, short, env)]
    refresh_time: Option<i32>,

    /// Path to temperature sensors configuration (default: sensors.yaml in the same loc)
    #[clap(long, short, env, default_value = "sensors.yaml")]
    sensors_config_path: PathBuf,

    /// The metrics API endpoint where to send the POST requests
    #[arg(long, short, env = "GRAPHITE_ENDPOINT")]
    endpoint: String,

    /// The API key to authenticate the POST requests
    #[arg(long, short, env = "GRAFANA_API_KEY")]
    apikey: String,
}

#[derive(Parser)]
struct CheckArguments {
    /// rovide GIO pin number the DHT22 sensor is connected to
    #[arg(long)]
    pin: u8,
}

#[derive(Serialize, Deserialize, Debug)]
struct Sensor {
    name: String,
    pin: u8,
}

#[derive(Serialize, Deserialize, Debug)]
struct Datapoint {
    name: String,
    interval: i32,
    value: f64,
    time: i64,
}

impl Datapoint {
    fn new(reading: &f32, label: &str, sensor: &Sensor, timestamp: u64, resolution: i32) -> Self {
        return Datapoint {
            name: format!("{}.{}", sensor.name, label),
            interval: resolution,
            value: f64::try_from(*reading).expect("Couldn't convert f32 to f64"),
            time: i64::try_from(timestamp).expect("Couldn't convert to i64 from u64"),
        };
    }
}

#[tokio::main]
async fn main() {
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();

    let args = Cli::parse();

    match args.command {
        Command::Serve(args) => handle_serve_command(args).await,
        Command::Check(args) => handle_check_command(args).await,
    };

    ()
}

async fn handle_check_command(args: CheckArguments) -> anyhow::Result<()> {
    let result = dht22_pi::read(args.pin as u8);
    match result {
        std::result::Result::Ok(reading) => {
            println!("{:?}", reading);
            Ok(())
        }

        Err(ReadingError::Checksum) => {
            eprintln!("Checksum value of the reading is incorrect!");
            Ok(())
        }

        Err(ReadingError::Timeout) => {
            eprintln!("Timeout reading the sensor value");
            Ok(())
        }

        Err(ReadingError::Gpio(error)) => {
            eprintln!("Problem reading GPIO value: {}", error);
            Ok(())
        }
    }
}

async fn handle_serve_command(args: ServeArguments) -> anyhow::Result<()> {
    let sensors = load_sensors_config(args.sensors_config_path).await;
    let refresh: i32 = if let Some(time) = args.refresh_time {
        time
    } else {
        DEFAULT_REFRESH_SECS
    };

    let mut refresh_interval = tokio::time::interval(time::Duration::from_secs(
        refresh.try_into().expect("Couldn't convert i32 to u64"),
    ));

    loop {
        refresh_interval.tick().await;
        let readings: Vec<Datapoint> =
            futures::future::join_all(sensors.iter().map(|sensor| async move {
                return read_sensor(sensor, refresh).await;
            }))
            .await
            .into_iter()
            .flatten()
            .collect();

        write_data(readings, &args.endpoint, &args.apikey).await;
    }
}

async fn write_data(readings: Vec<Datapoint>, endpoint: &str, apikey: &str) -> anyhow::Result<()> {
    let body = serde_json::to_string(&readings)?;

    log::info!("Sending a POST request to Grafana with: {}", &body);

    let client = reqwest::Client::new();
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .bearer_auth(apikey)
        .body(body)
        .send()
        .await?;

    log::info!("Received response: {:?}", &response);

    match response.status() {
        reqwest::StatusCode::OK => {
            log::info!("Data submitted to Graphite successfully!");
            return Ok(());
        }
        reqwest::StatusCode::FORBIDDEN => {
            log::error!("Unauthorized! Check the token.");
            return Ok(());
        }

        reqwest::StatusCode::BAD_REQUEST => {
            log::error!("Bad request!");
            return Ok(());
        }

        _ => {
            log::error!("Uncaught error writing data");
            return Ok(());
        }
    }
}

async fn read_sensor(sensor: &Sensor, resolution: i32) -> Vec<Datapoint> {
    let mut read_interval = tokio::time::interval(time::Duration::from_millis(2100));
    loop {
        read_interval.tick().await;

        // Try reading the sensor
        let result = dht22_pi::read(sensor.pin);

        // Handle the result
        match result {
            Ok(read) => {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System time behind Unix epoch time")
                    .as_secs();

                log::info!("Successfully read {:?}: {:?}", &sensor.name, &read);

                let temp_datapoint =
                    Datapoint::new(&read.temperature, "temperature", sensor, ts, resolution);
                let hum_datapoint =
                    Datapoint::new(&read.humidity, "humidity", sensor, ts, resolution);

                break vec![temp_datapoint, hum_datapoint];
            }

            Err(error) => {
                log::warn!("Error sensor read: {:?}", error);
                continue;
            }
        };
    }
}

async fn load_sensors_config(sensors_config_path: PathBuf) -> Vec<Sensor> {
    let sensors = {
        match fs::read_to_string(&sensors_config_path) {
            Ok(sensors) => sensors,
            Err(err) => {
                match err.kind() {
                    io::ErrorKind::NotFound => {
                        log::error!("sensors.yaml file not found ({})", err);
                    }
                    io::ErrorKind::PermissionDenied => {
                        log::error!(
                            "Insufficient permissions to read sensors.yaml file ({})",
                            err
                        );
                    }
                    _ => {
                        log::error!("Unable to read sensors.yaml file at: {}", err);
                    }
                };
                panic!("Exiting the application");
            }
        }
    };

    let sensors: Vec<Sensor> = serde_yaml::from_str(&sensors).expect("Invalid sensors YAML file"); // TODO: better errors for yaml

    return sensors;
}
