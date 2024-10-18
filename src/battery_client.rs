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

use anyhow::anyhow;
use bluest::Adapter;
use bluest::AdvertisingDevice;
use bluest::Characteristic;
use bluest::Device;
use bluest::Uuid;
use crc16::{State, MODBUS};
use futures_util::Stream;
use futures_util::StreamExt;
use tokio::time::timeout;
use tokio::time::Duration;

/// The reported state of the battery
#[derive(Debug)]
pub struct BatteryState {
    /// The state of charge of the battery in %
    pub state_of_charge_pct: u16,
    /// The residual capacity of the battery in Ah/100
    pub residual_capacity_cah: u16,
    pub cycles_count: u16,
    /// The voltage of each cell in mv. The N/A value is 61001
    pub cell_voltage_mv: Vec<u16>,
    /// The battery voltage in V/100
    pub battery_voltage_cv: u16,
}


pub struct BatteryClient {
    adapter: Adapter,
    device: Device,
    write: Characteristic,
    notify: Characteristic,
}

impl BatteryClient {
    const BLE_DEVICE_NAME: &'static str = "BT_HC6172";
    const NORDIC_UART_SERVICE_ID: &'static str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
    const NORDIC_UART_WRITE_CHARACTERISTIC_ID: &'static str =
        "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
    const NORDIC_UART_NOTIFY_CHARACTERISTIC_ID: &'static str =
        "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
    const MSG_HEADER: [u8; 2] = [0x01, 0x03];
    // A verbatim message to send which requests state of voltages
    const REQ_VOLTAGES: [u8; 8] = [0x01, 0x03, 0xd0, 0x00, 0x00, 0x26, 0xfc, 0xd0];
    // A verbatim message to send which requests the state of change and related data
    const REQ_SOC: [u8; 8] = [0x01, 0x03, 0xd0, 0x26, 0x00, 0x19, 0x5d, 0x0b];
    // How long to wait without any notifications before considering the message completely received
    const NOTIFICATION_TIMEOUT_S: u64 = 5;

    /// Disconnect from the battery
    pub async fn stop(self) -> anyhow::Result<()> {
        self.adapter.disconnect_device(&self.device).await?;
        Ok(())
    }

    pub async fn new_default_name() -> anyhow::Result<Self> {
        Self::new(Self::BLE_DEVICE_NAME).await
    }

    /// Create a new `BatteryClient`, which includes attempting to discover the device.
    pub async fn new(ble_device_name: &str) -> anyhow::Result<Self> {
        let adapter = bluest::Adapter::default()
            .await
            .ok_or(anyhow!("Default adapter not found"))?;
        adapter.wait_available().await?;

        let device = timeout(Duration::from_secs(30), Self::discover_device(ble_device_name, &adapter))
            .await
            .map_err(|_| anyhow!("Device not found"))??;

        adapter.connect_device(&device.device).await?;

        let nordic_uart_service = device
            .device
            .discover_services_with_uuid(Self::nordic_uart_service_id())
            .await?
            .first()
            .ok_or(anyhow!("The specified device does not support the Nordic UART service."))?
            .clone();
        let write = nordic_uart_service
            .discover_characteristics_with_uuid(Self::nordic_uart_write_characteristic_id())
            .await?
            .first()
            .ok_or(anyhow!("The specified device does not support the Nordic UART write characterstic."))?
            .clone();
        let notify = nordic_uart_service
            .discover_characteristics_with_uuid(Self::nordic_uart_notify_characteristic_id())
            .await?
            .first()
            .ok_or(anyhow!("The specified device does not support the Nordic UART notify characterstic."))?
            .clone();

        Ok(
            Self { adapter: adapter.clone(), device: device.device, write, notify }
        )
    }

    /// Read the current state from the battery
    pub async fn fetch_state(&mut self) -> anyhow::Result<BatteryState> {
        self.try_connect().await?;

        // let reader = self.notify.notify().await?;

        let rsp = self.request_response(&Self::REQ_SOC).await?;

        let nums: Vec<u16> = rsp
            .chunks(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect();

        println!("BATTERY SOC response: {nums:?}");

        let state_of_charge_pct = nums[14];
        let residual_capacity_cah = nums[16];
        let cycles_count = nums[19];

        let rsp = self.request_response(&Self::REQ_VOLTAGES).await?;
        let nums: Vec<u16> = rsp
            .chunks(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect();
        println!("BATTERY Voltages response: {nums:?}");

        let cell_voltage_mv = nums[0..32].to_vec();
        let battery_voltage_cv = nums[37];

        let state = BatteryState {
            state_of_charge_pct,
            residual_capacity_cah,
            cycles_count,
            cell_voltage_mv,
            battery_voltage_cv,
        };

        Ok(state)
    }

    async fn discover_device(name: &str, adapter: &Adapter) -> anyhow::Result<AdvertisingDevice> {
        let required_services =  [Self::nordic_uart_service_id()];
        let mut adapter_events = adapter.scan(&required_services).await?;
        while let Some(device) = timeout(Duration::from_secs(30), adapter_events.next()).await.map_err(|_| anyhow!("Device not found"))? {
            let device_name = device.device.name_async().await?;
            if device_name == name {
                return Ok(device)
            }
        }

        Err(anyhow!("Device not found"))
    }

    async fn request_response(&mut self, rq: &[u8]) -> anyhow::Result<Vec<u8>> {
        let reader = self.notify.notify().await?;

        let h = hex::encode(rq);
        println!("BATTERY: TX: {h}");

        self.write.write(rq).await?;

        let rsp = Self::read_message(reader).await?;

        Ok(rsp)
    }

    /// Attempt to read a whole message from the device.
    /// 
    /// Messages are delivered over multiple notification events. Although in theory it 
    /// is possible to know when you've received the whole message
    /// by using the message header information, that doesn't work in practice because
    /// you often get duplicated notifications as well as corrupted notifications.
    /// As a result, sometimes there are more notifcations to receive after the specifed
    /// message length has been reached and conversely, sometimes the notifcations
    /// stop before the specified message length is reached.
    /// 
    /// To deal with this a timeout mechanism is used. Notifications are read
    /// and appended to the received message until no more notifications are received 
    /// for a short time. Then the message is considered complete. If it is corrupted then
    /// that will be detected later during message parsing.
    /// 
    /// Unfortunately this introduces a minimum time to read a message of a few seconds.
    /// However, it is the only reliable way I've found.
    async fn read_message<T: Stream<Item = Result<Vec<u8>, bluest::Error>> + Send + Unpin>(mut reader: T) -> anyhow::Result<Vec<u8>> {
        // let mut reader = self.notify.notify().await?;
        let mut msg = Vec::<u8>::new();
        loop {
            let read_result =
                tokio::time::timeout(Duration::from_secs(Self::NOTIFICATION_TIMEOUT_S), reader.next()).await;

            match read_result {
                Err(_) => {
                    // timeout
                    let parse_msg_result = Self::try_parse_msg(&msg[..]);
                    match parse_msg_result {
                        TryParseMessageResult::Ok(payload) => return Ok(payload),
                        TryParseMessageResult::Incomplete => {
                            let h_msg = hex::encode(&msg[..]);
                            return Err(anyhow!("Message incomplete: {h_msg}"));
                        }
                        TryParseMessageResult::Invalid(e) => {
                            let h_msg = hex::encode(&msg[..]);
                            return Err(anyhow!("Message invalid: {e}: {h_msg}"));
                        }
                    }
                }
                Ok(None) => {
                    // End of stream

                    println!("BATTERY: End of notification stream");

                    return Err(anyhow!("end of notification stream"));
                }
                Ok(Some(Ok(data))) => {
                    let h_notification = hex::encode(&data);
                    println!("BATTERY: RX notification: 0x{h_notification}");

                    msg.extend_from_slice(&data);
                }
                Ok(Some(Err(err))) => {
                    println!("BATTERY: Notification error: {err}");

                    return Err(err.into());
                }
            }
        }
    }

    /// Attempt to parse the given message bytes returning the payload.
    /// 
    /// The message format is:
    /// 
    /// Start Byte | End Byte     | Meaning
    /// 0          | 1            | A constant header with value [0x01, 0x03]
    /// 2          | 2            | The length in bytes of the rest of the message after this byte
    /// 3          | x            | The payload
    /// x+1        | x+2          | A MODBUS CRC over the bytes 0-x
    fn try_parse_msg(buffer: &[u8]) -> TryParseMessageResult {
        if buffer.len() < 3 {
            return TryParseMessageResult::Incomplete;
        }

        let expected_header = &Self::MSG_HEADER[..];
        if &buffer[0..2] != expected_header {
            return TryParseMessageResult::Invalid("Unexpected header");
        }

        let expected_len = buffer[2] as usize + 5;
        if buffer.len() < expected_len {
            return TryParseMessageResult::Incomplete;
        }

        if buffer.len() > expected_len {
            return TryParseMessageResult::Invalid("Too long");
        }

        let crc_actual = &buffer[buffer.len() - 2..];
        let crc_expected = Self::crc(&buffer[0..buffer.len() - 2]);
        if crc_actual != crc_expected {
            return TryParseMessageResult::Invalid("CRC check failed");
        }

        let payload = buffer[3..buffer.len() - 2].to_vec();
        TryParseMessageResult::Ok(payload)
    }

    /// Compute the CRC check value for the given bytes
    fn crc(data: &[u8]) -> [u8; 2] {
        State::<MODBUS>::calculate(data).to_le_bytes()
    }

    fn nordic_uart_service_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_SERVICE_ID).unwrap()
    }

    fn nordic_uart_write_characteristic_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_WRITE_CHARACTERISTIC_ID).unwrap()
    }

    fn nordic_uart_notify_characteristic_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_NOTIFY_CHARACTERISTIC_ID).unwrap()
    }

    async fn try_connect(&self) -> anyhow::Result<()> {
        if !self.device.is_connected().await {
            let mut retries = 2;
            loop {
                match self.adapter.connect_device(&self.device).await {
                    Ok(()) => return Ok(()),
                    Err(err) if retries > 0 => {
                        println!("BATTERY: Failed to connect: {err}");
                        retries -= 1;
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }

        Ok(())
    }
}

#[test]
fn test_try_parse_message_happy() {
    let message =
        hex::decode("010318240c000002a7000000000000000000000000000000000000bc90").unwrap();
    let payload = hex::decode("240c000002a7000000000000000000000000000000000000").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Ok(payload));
}

#[test]
fn test_try_parse_message_no_header() {
    let message = hex::decode("0103").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Incomplete);
}

#[test]
fn test_try_parse_message_incomplete() {
    let message = hex::decode("010318240c000002a700000000000000000000000000000000bc").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Incomplete);
}

#[test]
fn test_try_parse_message_bad_crc() {
    let message =
        hex::decode("010318240c000002a7000000000000000000000000000000000000bc91").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Invalid("CRC check failed"));
}

#[derive(PartialEq, Eq, Debug)]
enum TryParseMessageResult {
    Ok(Vec<u8>),
    Incomplete,
    Invalid(&'static str),
}

#[test]
fn test_checksum() {
    let payload = [
        0x01, 0x03, 0x18, 0x24, 0x0c, 0x00, 0x00, 0x02, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let expected = 0x90bc;
    assert_eq!(State::<MODBUS>::calculate(&payload), expected);
}
