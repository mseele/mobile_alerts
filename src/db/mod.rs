mod schema;

use chrono::{DateTime, Utc};
use diesel::dsl::exists;
use diesel::pg::PgConnection;
use diesel::result::Error;
use diesel::{insert_into, prelude::*, select, Identifiable, Insertable, Queryable};
use schema::devices;
use schema::measurements;
use std::env;

#[derive(Identifiable, Queryable, Clone, Debug)]
#[diesel(table_name = devices)]
pub struct Device {
    pub id: i32,
    pub device_id: String,
    pub name: String,
    pub alert: bool,
}

#[derive(Identifiable, Queryable, Associations, Debug)]
#[diesel(table_name = measurements)]
#[diesel(belongs_to(Device))]
pub struct Measurement {
    pub id: i32,
    pub device_id: i32,
    pub time: DateTime<Utc>,
    pub temperature: f64,
    pub humidity: f64,
    pub temperature_outside: Option<f64>,
    pub humidity_outside: Option<f64>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = measurements)]
pub struct NewMeasurement<'a> {
    pub device_id: i32,
    pub time: &'a DateTime<Utc>,
    pub temperature: f64,
    pub humidity: f64,
    pub temperature_outside: Option<f64>,
    pub humidity_outside: Option<f64>,
}

impl NewMeasurement<'_> {
    pub fn new(
        device_id: i32,
        time: &DateTime<Utc>,
        temperature: f64,
        humidity: f64,
        temperature_outside: Option<f64>,
        humidity_outside: Option<f64>,
    ) -> NewMeasurement {
        NewMeasurement {
            device_id,
            time,
            temperature,
            humidity,
            temperature_outside,
            humidity_outside,
        }
    }
}

pub fn establish_connection() -> PgConnection {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

pub fn fetch_devices(connection: &mut PgConnection) -> Vec<Device> {
    use schema::devices::dsl::*;

    devices
        .load::<Device>(connection)
        .expect("Error loading devices")
}

pub fn measurement_exists(
    connection: &mut PgConnection,
    id: i32,
    timestamp: &DateTime<Utc>,
) -> Result<bool, Error> {
    use schema::measurements::dsl::{device_id, measurements, time};

    select(exists(
        measurements.filter(device_id.eq(id).and(time.eq(timestamp))),
    ))
    .get_result(connection)
}

pub fn insert_measurement(
    connection: &mut PgConnection,
    measurement: &NewMeasurement,
) -> Result<usize, Error> {
    use schema::measurements::dsl::*;

    insert_into(measurements)
        .values(measurement)
        .execute(connection)
}

pub fn get_measurements<'a>(
    connection: &mut PgConnection,
    devices: &'a Vec<Device>,
    measurement_count: i64,
) -> Result<Vec<(&'a Device, Vec<Measurement>)>, Error> {
    use schema::measurements::dsl::*;

    let grouped_measurements: Vec<Vec<Measurement>> = Measurement::belonging_to(devices)
        .limit(measurement_count)
        .order(time.desc())
        .load::<Measurement>(connection)?
        .grouped_by(devices);
    let result = devices
        .iter()
        .zip(grouped_measurements)
        .collect::<Vec<_>>();

    Ok(result)
}
