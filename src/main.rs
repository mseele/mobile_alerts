#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate log;

mod db;

use chrono::{DateTime, NaiveDateTime, Utc};
use dotenv::dotenv;
use log::{error, info, trace};
use serde::Deserialize;
use std::env;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Deserialize, Debug)]
struct APIResponse {
    devices: Vec<Device>,
    success: bool,
}

#[derive(Deserialize, Debug)]
struct Device {
    deviceid: String,
    // lastseen: u32,
    // lowbattery: bool,
    measurement: Measurement,
}

#[derive(Deserialize, Debug)]
struct Measurement {
    // idx: u32,
    ts: u32,
    // c: u32,
    // lb: bool,
    t1: f64,
    t2: Option<f64>,
    h: f64,
    h2: Option<f64>,
}

fn is_window_open(latest_temperature: &f64, temperature: &f64) -> bool {
    trace!(
        "calculate: {} - {} = {}",
        temperature,
        latest_temperature,
        temperature - latest_temperature
    );
    temperature - latest_temperature >= 2.0
}

fn send_notification(device_name: &str) -> Result<(), ureq::Error> {
    let app_key = env::var("APP_KEY").expect("APP_KEY must be set");
    let app_secret = env::var("APP_SECRET").expect("APP_SECRET must be set");
    let message = format!("Das Fenster im {} ist noch offen", device_name);
    let params = [
        ("app_key", app_key.as_str()),
        ("app_secret", app_secret.as_str()),
        ("target_type", "app"),
        ("content", message.as_str()),
    ];
    ureq::post("https://api.pushed.co/1/push").send_form(&params)?;
    Ok(())
}

fn check(temperatures: &Vec<f64>, device_name: &str) -> Result<(), ureq::Error> {
    match temperatures.first() {
        Some(latest_temperature) => {
            for temperature in temperatures.iter().skip(1) {
                if is_window_open(latest_temperature, temperature) {
                    info!(
                        "send alert for room {} (latest temp: {} / temp: {})",
                        device_name, latest_temperature, temperature
                    );
                    send_notification(device_name)?;
                    break;
                }
            }
            Ok(())
        }
        None => Ok(()),
    }
}

pub fn run() -> Result<(), ureq::Error> {
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
    let body = format!("phoneid={}&deviceids={}", phone_id, device_ids);
    trace!("request data for {}", body);
    let data = ureq::post("https://www.data199.com/api/pv1/device/lastmeasurement")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(body.as_str())?
        .into_json::<APIResponse>()?;

    if !data.success {
        error!("mobile_alerts request was not successful: {:?}", data);
        return Ok(());
    }

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
                                Ok(_) => trace!("new measurement has been inserted into database: {:?}", new_measurement),                                
                                Err(e) => error!("inserting a new measurement ({:?}) failed: {}", new_measurement, e),
                            }
                            if device.alert {
                                devices_to_check.push(device.clone());
                            }
                        }
                    }
                    Err(e) => error!("check for existence of measurement with device_id={} and time={} failed: {}", id, time, e),
                }
            }
            None => error!(
                "find no matching device for result device {:?}",
                measurement_device
            ),
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
        Err(e) => error!(
            "getting measurements for devices ({:?}) failed: {}",
            devices_to_check, e
        ),
    }

    for value in temperatures {
        check(&value.1, value.0)?;
    }

    Ok(())
}

fn main() {
    dotenv().ok();
    env_logger::init();

    trace!("starting up");

    // create a channel to handle the application shutdown
    let (shutdown, shutdown_receiver) = mpsc::channel();

    // listen to ctrl_c and stop the application by sending a signal to the channel
    ctrlc::set_handler(move || {
        trace!("received ctrl_c");
        shutdown.send(()).expect("failed to send shutdown event");
    })
    .expect("failed to listen for ctrl_c");

    // continuously execute the run method until the shutdown signal will be send
    let duration = Duration::from_secs(60);
    loop {
        let result = run();
        if result.is_err() {
            error!("failure while running the logic: {:?}", result.err());
        }
        match shutdown_receiver.recv_timeout(duration) {
            Ok(_) => break,
            Err(e) => trace!("waiting for shutdown_receiver failed: {}", e),
        }
    }

    trace!("shutdown");
}
