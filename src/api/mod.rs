use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};

pub mod frontend;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Data {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub sensors: Sensors,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Sensors {
    pub score: u8,
    pub dew_point: f32,
    pub temp: f32,
    pub humid: f32,
    pub abs_humid: f32,
    pub co2: u32,
    pub co2_est: u32,
    pub co2_est_baseline: u32,
    pub voc: u32,
    pub voc_baseline: u32,
    pub voc_h2_raw: u32,
    pub voc_ethanol_raw: u32,
    pub pm25: u32,
    pub pm10_est: u32,
}
