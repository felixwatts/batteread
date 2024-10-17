use anyhow::anyhow;
use bluest::Adapter;
use bluest::AdvertisingDevice;
use bluest::Characteristic;
use bluest::Service;
use bluest::Uuid;
use crc16::{State, MODBUS};
use futures_util::StreamExt;
use tokio::time::Duration;

// This code reads some status data from a LiFePo4 battery manufactured by Li-ion and sold around the year 2022
//
// The BMS has a BLE (bluetooth) interface. On top of that the NordicUART protocol is used for serial communication.
// On top of that there seems to be a proprietary request-response protocol which I have attempted to partially
// reverse engineer.
//
// Details of the proprietary protocol:
//
// The server waits for requests on the NordicUART WRITE characteristic. It sends responses via the NordicUART NOTIFY characteristic.
//
// Messages from client to server (requests), I do not understand the structure of these but I just send verbatim what I observed
// the official android client sending.
//
// Messages from the server to the client are sent in response to requests. Each response may be split over several
// NordicUART notifications. The message structure is:
//
// [ 2 bytes: header ] [ 1 byte: payload length P ] [ P bytes: payload ] [ 2 bytes checksum ]
//
// The header is always [ 0x01, 0x03 ] for the requests I send.
// All numbers are big endian, unsigned.
// The checksum is a MODBUS checksum of the whole of the message up to the start of the checksum, but the two bytes are reversed.
//
// It is necessary to check the checksum as messages are quite commonly corrupted.
//
// There are two types of request-response that I use:
//
// VOLTAGES
//
// Request: 0x0103d0000026fcd0
// Response:
// bytes 0-64: cell voltages in mV, 32 * u16
// bytes 76-77: battery voltage in mv, u16
//
// STATE_OF_CHARGE
//
// Request: 0x0103d02600195d0b
// Response:
//  bytes 28-29: State of charge in %, u16
//  bytes 32-33: Residual capacity in mAh, u16
//  bytes 38-39: Cycles (count), u16

// Failed rq/rsp:
//
// TX: 0103d0000026fcd0
// RX notification
// "01034c0d7e0d7c0d6b0d790d7b0d7e0d7c0d7f"
// Message INCOMPLETE
// RX notification
// "01034c0d7e0d7c0d6b0d790d7b0d7e0d7c0d7fee49ee49ee49ee49ee49ee49ee49ee49ee49ee49"
// Message INCOMPLETE
// RX notification
// "01034c0d7e0d7c0d6b0d790d7b0d7e0d7c0d7fee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49"
// Message INCOMPLETE
// RX notification
// "01034c0d7e0d7c0d6b0d790d7b0d7e0d7c0d7fee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee490d7f0d6b0008000300140ac7"
// Message INCOMPLETE

// 0d7e0d7c0d6b0d790d7b0d7e0d7c0d7fee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee490d7f0d6b0008000300140ac7

#[derive(Debug)]
pub struct BatteryState {
    pub state_of_charge_pct: u16,
    pub residual_capacity_cah: u16,
    pub cycles_count: u16,
    pub cell_voltage_mv: Vec<u16>,
    pub battery_voltage_cv: u16,
}

pub struct BatteryClient {
    adapter: Adapter,
    device: AdvertisingDevice,
    write: Characteristic,
    notify: Characteristic,
}

// 6e400002-b5a3-f393-e0a9-e50e24dcca9e WRITE_WITHOUT_RESPONSE | WRITE : UART write?
// 6e400003-b5a3-f393-e0a9-e50e24dcca9e NOTIFY : UART read?

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

    pub async fn new() -> anyhow::Result<Self> {

        let adapter = Adapter::default().await.ok_or(anyhow!("Bluetooth adapter not found"))?;
        adapter.wait_available().await?;
    
        let filter = [Self::nordic_uart_service_id()];
        let mut scan = adapter.scan(&filter).await?;

        let device = loop {
            let next = scan.next().await;
            if let Some(device) = next{
                if device.device.name().as_deref().unwrap_or("(unknown)") == Self::BLE_DEVICE_NAME {
                    break device;
                }
            }
        };

        drop(scan);

        let services = device.device.services().await?;
        let write = Self::find_characteristic(&services, Self::nordic_uart_write_characteristic_id())
            .await?
            .ok_or(anyhow!("Cannot find Nordic UART write characteristic"))?;
        let notify = Self::find_characteristic(&services, Self::nordic_uart_notify_characteristic_id())
            .await?
            .ok_or(anyhow!("Cannot find Nordic UART notify characteristic"))?;

        Ok(
            Self{
                adapter,
                device,
                write,
                notify
            }
        )
    }

    pub async fn fetch_state(&mut self) -> anyhow::Result<BatteryState> {

        self.try_connect().await?;

        self.write_msg(&Self::REQ_SOC).await?;
        let rsp = self.read_message().await?;
        let nums: Vec<u16> = rsp
            .chunks(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect();

        println!("BATTERY SOC response: {nums:?}");

        let state_of_charge_pct = nums[14];
        let residual_capacity_cah = nums[16];
        let cycles_count = nums[19];

        self.write_msg(&Self::REQ_VOLTAGES).await?;
        let rsp = self.read_message().await?;
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

    async fn write_msg(&mut self, full_msg_bytes: &[u8]) -> anyhow::Result<()> {
        let h = hex::encode(full_msg_bytes);
        println!("BATTERY: TX: {h}");

        self.write.write(full_msg_bytes).await?;

        Ok(())
    }

    async fn read_message(&self) -> anyhow::Result<Vec<u8>> {
        let mut reader = self.notify.notify().await?;
        let mut msg = Vec::<u8>::new();

        loop {
            let read_result =
                tokio::time::timeout(Duration::from_secs(15), reader.next()).await;

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

    async fn try_connect(&self) -> anyhow::Result<()> {
        if !self.device.device.is_connected().await {
            let mut retries = 2;
            loop {
                match self.adapter.connect_device(&self.device.device).await {
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

    fn nordic_uart_service_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_SERVICE_ID).unwrap()
    }

    fn nordic_uart_write_characteristic_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_WRITE_CHARACTERISTIC_ID).unwrap()
    }

    fn nordic_uart_notify_characteristic_id() -> Uuid {
        Uuid::parse_str(Self::NORDIC_UART_NOTIFY_CHARACTERISTIC_ID).unwrap()
    }

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

    fn crc(data: &[u8]) -> [u8; 2] {
        let crc_bytes_reversed = State::<MODBUS>::calculate(data).to_be_bytes();
        [crc_bytes_reversed[1], crc_bytes_reversed[0]]
    }

    async fn find_characteristic(
        services: &[Service],
        char_id: Uuid,
    ) -> anyhow::Result<Option<Characteristic>> {

        for s in services.iter() {
            let characteristics = s.characteristics().await?;
            for c in characteristics.iter() {
                if c.uuid() == char_id {
                    return Ok(Some(c.clone()))
                }
            }
        }

        Ok(None)
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
