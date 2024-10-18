//! Read status data from certain models of LiFePO4 Battery Management Systems over Bluetooth Low Energy
//! 
//! Tested with a 400ah 24v battery manufactured by <https://www.li-gen.net/> and sold around the year 2022.
//! 
//! The BMS has a BLE interface. On top of that the NordicUART protocol is used for serial communication.
//! On top of that there seems to be a proprietary request-response protocol which I have attempted to partially
//! reverse engineer.
//! 
//! Currently the following data can be accessed:
//! 
//! - State of charge (%)
//! - Residual capacity (Ah)
//! - Cycles (count)
//! - Cell voltages (v)
//! - Battery voltage (v)
//! 
//! # Example
//! 
//! ```rust
//! # use std::time::Duration;
//! #
//! # #[tokio::main]
//! # pub async fn main(){
//!     let mut battery_client = batteread::BatteryClient::new_default_name().await.unwrap();
//!     loop {
//!         let battery_state = battery_client.fetch_state().await.unwrap();
//!         println!("{battery_state:?}");
//!         tokio::time::sleep(Duration::from_secs(5)).await;
//!     }
//! # }
//! ```

mod battery_client;
mod battery_state;
mod message;

pub use battery_client::BatteryClient;
pub use battery_state::BatteryState;