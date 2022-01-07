#[macro_use]
extern crate diesel;
extern crate dotenv;

mod db;

use std::env;

use chrono::{DateTime, NaiveDateTime, Utc};
use dotenv::dotenv;
use serde::Deserialize;
use tokio::signal;
use tokio::time::{interval, Duration};

#[derive(Deserialize, Debug)]
struct APIResponse {
    devices: Vec<Device>,
    success: bool,
}

#[derive(Deserialize, Debug)]
struct Device {
    deviceid: String,
    lastseen: u32,
    lowbattery: bool,
    measurement: Measurement,
}

#[derive(Deserialize, Debug)]
struct Measurement {
    idx: u32,
    ts: u32,
    c: u32,
    lb: bool,
    t1: f64,
    t2: Option<f64>,
    h: f64,
    h2: Option<f64>,
}

fn is_window_open(latest_temperature: &f64, temperature: &f64) -> bool {
    println!(
        "{} - {} = {}",
        temperature,
        latest_temperature,
        temperature - latest_temperature
    );
    temperature - latest_temperature >= 2.0
}

async fn send_notification(device_name: &str) -> Result<(), reqwest::Error> {
    let app_key = env::var("APP_KEY").expect("APP_KEY must be set");
    let app_secret = env::var("APP_SECRET").expect("APP_SECRET must be set");
    let message = format!("Das Fenster im {} ist noch offen", device_name);
    let params = [
        ("app_key", app_key.as_str()),
        ("app_secret", app_secret.as_str()),
        ("target_type", "app"),
        ("content", message.as_str()),
    ];
    match reqwest::Client::new()
        .post("https://api.pushed.co/1/push")
        .form(&params)
        .send()
        .await?
        .error_for_status()
    {
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn check(temperatures: &Vec<f64>, device_name: &str) -> Result<(), reqwest::Error> {
    match temperatures.first() {
        Some(latest_temperature) => {
            for temperature in temperatures.iter().skip(1) {
                if is_window_open(latest_temperature, temperature) {
                    // FIXME: remove
                    println!("ALERT {:?} {:?}", device_name, temperature);
                    send_notification(device_name).await?;
                    break;
                }
            }
            Ok(())
        }
        None => Ok(()),
    }
}

pub async fn run() -> Result<(), reqwest::Error> {
    // establish database connection and fetch devices
    let connection = db::establish_connection();
    let devices = db::fetch_devices(&connection);

    // get phone id and concat device ids for the http request
    let phone_id = env::var("PHONE_ID").expect("PHONE_ID must be set");
    let device_ids = devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect::<Vec<&str>>()
        .join(",");

    // request device data
    let data = reqwest::Client::new()
        .post("https://www.data199.com/api/pv1/device/lastmeasurement")
        .body(format!("phoneid={}&deviceids={}", phone_id, device_ids))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await?
        .json::<APIResponse>()
        .await?;

    let mut devices_to_check: Vec<db::Device> = Vec::new();

    for measurement_device in data.devices.iter() {
        let device = devices
            .iter()
            .find(|&device| &device.device_id == &measurement_device.deviceid);
        match device {
            Some(device) => {
                let id = device.id;
                let measurement = &measurement_device.measurement;

                let time = DateTime::<Utc>::from_utc(
                    NaiveDateTime::from_timestamp(measurement.ts.into(), 0),
                    Utc,
                );

                match db::measurement_exists(&connection, id, &time) {
                    Ok(exists) => {
                        if !exists {
                            let new_measurement = db::NewMeasurement::new(
                                id,
                                &time,
                                measurement.t1,
                                measurement.h,
                                measurement.t2,
                                measurement.h2,
                            );
                            match db::insert_measurement(&connection, &new_measurement) {
                                Ok(_) => (),
                                Err(e) => todo!("handle error"),
                            }
                            if device.alert {
                                devices_to_check.push(device.clone());
                            }
                        }
                    }
                    Err(e) => todo!("handle error"),
                }
            }
            None => (),
        }
    }

    let mut temperatures: Vec<(&String, Vec<f64>)> = Vec::new();

    match db::get_measurements(&connection, &devices_to_check) {
        Ok(values) => {
            for value in values {
                temperatures.push((
                    &value.0.name,
                    value.1.iter().map(|m| m.temperature).collect::<Vec<_>>(),
                ));
            }
        }
        Err(_) => todo!("handle error"),
    }

    for value in temperatures {
        check(&value.1, value.0).await?;
    }

    Ok(())
}

async fn manage_timer() {
    let mut interval = interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let result = run().await;
        if result.is_err() {
            println!("Error: {:?}", result.err());
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    // start a timer
    tokio::spawn(manage_timer());
    // wait until ctrl_c has been pressed
    signal::ctrl_c().await.expect("failed to listen for ctrl_c");
}
