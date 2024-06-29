use std::pin::Pin;
use btleplug::api::{Central, CharPropFlags, Characteristic, Peripheral, ScanFilter, ValueNotification, WriteType};
use btleplug::api::Manager as _;
use btleplug::platform::Manager;
use crc16::{State, MODBUS};
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use futures_util::{Stream, StreamExt};

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
// The checksum is a MODBUS checksum of the whole of the message up to the start of the checksum.
//
// It is neccesary to check the checksum as messages are quite commonly corrupted.
//
// There are two types of request-reponse that I use:
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

#[derive(Debug)]
pub (crate) struct BatteryState{
    pub (crate) state_of_charge_pct: u16,
    pub (crate) residual_capacity_mah: u16,
    pub (crate) cycles_count: u16,
    pub (crate) cell_voltage_mv: Vec<u16>,
    pub (crate) battery_voltage_mv: u16
}

pub (crate) struct BatteryClient{
    peripheral: btleplug::platform::Peripheral,
    notifications: Pin<Box<dyn Stream<Item=ValueNotification>>>
}

    // 6e400002-b5a3-f393-e0a9-e50e24dcca9e WRITE_WITHOUT_RESPONSE | WRITE : UART write?
    // 6e400003-b5a3-f393-e0a9-e50e24dcca9e NOTIFY : UART read?

impl BatteryClient{
    const BLE_DEVICE_NAME: &'static str = "BT_HC6172";
    const NORDIC_UART_SERVICE_ID: &'static str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
    const NORDIC_UART_WRITE_CHARACTERISTIC_ID: &'static str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
    const NORDIC_UART_NOTIFY_CHARACTERISTIC_ID: &'static str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
    const MSG_HEADER: [u8;2] = [0x01, 0x03];
    // A verbatim message to send which requests state of voltages
    const REQ_VOLTAGES: [u8; 8] = [0x01, 0x03, 0xd0, 0x00, 0x00, 0x26, 0xfc, 0xd0];
    // A verbatim message to send which requests the state of change and related data
    const REQ_SOC: [u8; 8] = [0x01, 0x03, 0xd0, 0x26, 0x00, 0x19, 0x5d, 0x0b];

    pub (crate) async fn new() -> Result<Self, String>{
        // Initialize the Bluetooth manager
        let manager = Manager::new().await.unwrap();

        // Get the first Bluetooth adapter
        let adapters = manager.adapters().await.unwrap();
        let central = adapters.into_iter().nth(0).ok_or("No Bluetooth adapter found")?;

        // Start scanning for devices
        central.start_scan(ScanFilter::default()).await.unwrap();

        println!("Begin scan..");
        sleep(Duration::from_secs(30)).await; // Allow some time to discover devices
        println!("Scan complete");

        // Find the specified device by name
        let peripherals = central.peripherals().await.unwrap();
        let peripheral = Self::find_peripheral(peripherals).await.ok_or("Bluetooth device not found")?;

        // Connect to the device
        peripheral.connect().await.map_err(|_| "Failed to connect to peripheral")?;
        peripheral.discover_services().await.map_err(|_| "Failed to discover peripheral services")?;

        let notifications = peripheral.notifications().await.map_err(|_| "Failed to get peripheral notifications")?;

        peripheral.subscribe(&Self::nordic_uart_notify_characteristic()).await.map_err(|_| "Failed to subscribe for notify characteristic")?;

        println!("Battery client is up");
        
        Ok(Self{
            peripheral,
            notifications
        })
    }

    pub (crate) async fn fetch_state(&mut self) -> Result<BatteryState, String> {
        self.write_msg(&Self::REQ_SOC).await?;
        let rsp = self.read_message().await?;

        let state_of_charge_pct = u16::from_be_bytes([rsp[28], rsp[29]]);
        let residual_capacity_mah = u16::from_be_bytes([rsp[32],rsp[33]]);
        let cycles_count = u16::from_be_bytes([rsp[38],rsp[39]]);

        self.write_msg(&Self::REQ_VOLTAGES).await?;
        let rsp = self.read_message().await?;

        let nums: Vec<u16> = rsp.chunks(2).map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]])).collect();

        let cell_voltage_mv = nums[0..32].to_vec();
        let battery_voltage_mv = nums[38];

        let state = BatteryState{
            state_of_charge_pct,
            residual_capacity_mah,
            cycles_count,
            cell_voltage_mv,
            battery_voltage_mv
        };

        Ok(state)

    }

    async fn write_msg(&mut self, full_msg_bytes: &[u8]) -> Result<(), String> {
        self.peripheral.write(
            &Self::nordic_uart_write_characteristic(), 
            &full_msg_bytes, 
            WriteType::WithResponse
        ).await.map_err(|_| "Failed to write")?;
        Ok(())
    }

    async fn read_message(&mut self) -> Result<Vec<u8>, String> {
        let mut buf = vec![];
        loop {
            
            let notification = self.notifications.next().await.ok_or("Failed to receive expected notification")?;

            println!("RX notification");
            
            assert!(notification.uuid == Self::nordic_uart_notify_characteristic().uuid);

            buf.extend(notification.value);

            let msg_result = Self::try_parse_msg(&buf);

            println!("{buf:?}");

            match msg_result {
                TryParseMessageResult::Ok(payload) => {
                    println!("Message COMPLETE");
                    return Ok(payload)
                },
                TryParseMessageResult::Invalid => {
                    println!("Message INVALID");
                    return Err("Invalid message".into())
                },
                TryParseMessageResult::Incomplete => {
                    println!("Message INCOMPLETE");
                }
            }
        }
    }

    async fn find_peripheral(peripherals: Vec<btleplug::platform::Peripheral>) -> Option<btleplug::platform::Peripheral> {
        for p in peripherals.into_iter() {
            let local_name = p.properties().await.ok().flatten().map(|p| p.local_name).flatten();
            match local_name {
                Some(name) if name == Self::BLE_DEVICE_NAME => return Some(p),
                _ => {}
            }
        }
        return None
    }

    fn nordic_uart_write_characteristic() -> Characteristic {
        Characteristic {
            uuid: Uuid::parse_str(Self::NORDIC_UART_WRITE_CHARACTERISTIC_ID).unwrap(),
            service_uuid: Uuid::parse_str(Self::NORDIC_UART_SERVICE_ID).unwrap(),
            properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::WRITE,
            descriptors: Default::default()
        }
    }
    
    fn nordic_uart_notify_characteristic() -> Characteristic {
        Characteristic{
            uuid: Uuid::parse_str(Self::NORDIC_UART_NOTIFY_CHARACTERISTIC_ID).unwrap(),
            service_uuid: Uuid::parse_str(Self::NORDIC_UART_SERVICE_ID).unwrap(),
            properties: CharPropFlags::NOTIFY,
            descriptors: Default::default()
        }
    }

    fn try_parse_msg(buffer: &[u8]) -> TryParseMessageResult{
        if buffer.len() < 3 { 
            return TryParseMessageResult::Incomplete 
        }

        let expected_header = &Self::MSG_HEADER[..];
        if &buffer[0..2] != expected_header {
            return TryParseMessageResult::Invalid
        }

        let expected_len = buffer[2] as usize + 5;
        if buffer.len() < expected_len {
            return TryParseMessageResult::Incomplete;
        }

        let crc_actual = &buffer[buffer.len()-2..];
        let crc_expected = Self::crc(&buffer[0..buffer.len()-2]);
        if crc_actual != crc_expected {
            return TryParseMessageResult::Invalid
        }

        let payload = buffer[3..buffer.len()-2].to_vec();
        TryParseMessageResult::Ok(payload)
    }

    fn crc(data: &[u8]) -> [u8;2] {
        let crc_bytes_reversed = State::<MODBUS>::calculate(&data).to_be_bytes();
        [crc_bytes_reversed[1], crc_bytes_reversed[0]]
    }
}

#[test]
fn test_try_parse_message_happy() {
    let message = hex::decode("010318240c000002a7000000000000000000000000000000000000bc90").unwrap();
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
    let message = hex::decode("010318240c000002a7000000000000000000000000000000000000bc").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Incomplete);
}

#[test]
fn test_try_parse_message_bad_crc() {
    let message = hex::decode("010318240c000002a7000000000000000000000000000000000000bc91").unwrap();
    let result = BatteryClient::try_parse_msg(&message[..]);
    assert_eq!(result, TryParseMessageResult::Invalid);
}

#[derive(PartialEq, Eq, Debug)]
enum TryParseMessageResult{
    Ok(Vec<u8>),
    Incomplete,
    Invalid
}

#[test]
fn test_checksum() {
    let payload = [0x01, 0x03, 0x18, 0x24,0x0c,0x00,0x00,0x02,0xa7,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00];
    let expected = 0x90bc;
    assert_eq!(State::<MODBUS>::calculate(&payload), expected);
}