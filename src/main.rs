use bluetooth_serial_port_async::{BtAddr, BtProtocol, BtSocket};
// use tokio_modbus::prelude::*;
// use btleplug::api::{Central, CharPropFlags, Peripheral, ScanFilter};
// use btleplug::api::Manager as _;
// use btleplug::platform::Manager;

// // Could be useful:
// // https://github.com/FurTrader/OverkillSolarBMS/blob/master/Comm_Protocol_Documentation/JBD_REGISTER_MAP.md

// const DEVICE_NAME: &'static str = "BT_HC6172";
// const DEVICE_MAC_ADDRESS: BtAddr = BtAddr([0xC3,0x7A,0x68,0x17,0x6B,0xFC]);
// const REG_BASIC_INFO: u16 = 0x03;
// const LEN_BASIC_INFO: u16 = 1;

// #[tokio::main]
// async fn main() {
//     // print_bms_details().await;
//     print_bms_state().await;
// }

// async fn print_bms_state() {
//     // let devices = bluetooth_serial_port_async::scan_devices(std::time::Duration::from_secs(20)).unwrap();
//     // if devices.len() == 0 {
//     //     panic!("No devices found");
//     // }

//     // println!("Found bluetooth devices {:?}", devices);

//     // let device = devices.iter().find(|d| d.name == DEVICE_NAME).expect("BMS device not found");

//     let mut socket = BtSocket::new(BtProtocol::RFCOMM).unwrap();
//     socket.connect(DEVICE_MAC_ADDRESS).unwrap();

//     // let stream = socket.get_stream();

//     // // let slave = Slave(0x01);
//     // let mut ctx = rtu::attach(stream);

//     // println!("Reading BASIC_INFO");
//     // let rsp = ctx.read_holding_registers(REG_BASIC_INFO, LEN_BASIC_INFO).await.unwrap().unwrap();
//     // println!("BASIC_INFO value is: {rsp:?}");

//     // let pack_voltage = (rsp[0] as f32) / 100.0;
//     // println!("Pack Voltage is {pack_voltage:.2}V");

//     // println!("Disconnecting");
//     // ctx.disconnect().await.unwrap().unwrap();
// }

// use bluetooth_serial_port::{BtProtocol, BtSocket};
// use std::io::{Read, Write};
// use std::time;
// use mio::{Poll, Token, Interest};

// fn main() {
//     scan for devices
//     let devices = bluetooth_serial_port::scan_devices(time::Duration::from_secs(20)).unwrap();
//     if devices.len() == 0 {
//         panic!("No devices found");
//     }

//     println!("Found bluetooth devices {:?}", devices);

//     // "device.addr" is the MAC address of the device
//     let device = devices.into_iter().find(|d| d.name == "BT_HC6172").expect("Device not found");
//     println!(
//         "Connecting to `{}` ({})",
//         device.name,
//         device.addr.to_string()
//     );

//     // create and connect the RFCOMM socket
//     let mut socket = BtSocket::new(BtProtocol::RFCOMM).unwrap();
//     socket.connect(device.addr).unwrap();

//     // BtSocket implements the `Read` and `Write` traits (they're blocking)
//     let mut buffer = [0; 10];
//     let num_bytes_read = socket.read(&mut buffer[..]).unwrap();
//     let num_bytes_written = socket.write(&buffer[0..num_bytes_read]).unwrap();
//     println!(
//         "Read `{}` bytes, wrote `{}` bytes",
//         num_bytes_read, num_bytes_written
//     );

//     // BtSocket also implements `mio::Evented` for async IO
//     let poll = Poll::new().unwrap();
//     poll.registry().register(&mut socket, Token(0), Interest::READABLE | Interest::WRITABLE).unwrap();
//     // loop { ... poll events and wait for socket to be readable/writable ... }
// }

// struct BtSocketEventSource{
//     wrapped: BtSocket
// }

// impl mio::event::Source for BtSocketEventSource{
//     fn register(
//         &mut self,
//         registry: &mio::Registry,
//         token: Token,
//         interests: Interest,
//     ) -> std::io::Result<()> {
//         self.wrapped.register(registry, token, interests)
//     }

//     fn reregister(
//         &mut self,
//         registry: &mio::Registry,
//         token: Token,
//         interests: Interest,
//     ) -> std::io::Result<()> {
//         todo!()
//     }

//     fn deregister(&mut self, registry: &mio::Registry) -> std::io::Result<()> {
//         todo!()
//     }
// }

use btleplug::api::{Central, CharPropFlags, Peripheral, ScanFilter};
use btleplug::api::Manager as _;
use btleplug::platform::Manager;
// use tokio_modbus::prelude::*;
use tokio::time::{sleep, Duration};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Initialize the Bluetooth manager
    let manager = Manager::new().await.unwrap();

    // Get the first Bluetooth adapter
    let adapters = manager.adapters().await.unwrap();
    let central = adapters.into_iter().nth(0).expect("No Bluetooth adapter found");

    // Start scanning for devices
    central.start_scan(ScanFilter::default()).await.unwrap();

    println!("Scanning for 60s");

    sleep(Duration::from_secs(60)).await; // Allow some time to discover devices

    println!("Finished scanning");

    // Find the specified device by name
    let device_name = "BT_HC6172";
    let peripherals = central.peripherals().await.unwrap();

    println!("{peripherals:?}");

    let peripheral = find_peripheral(peripherals, device_name).await.expect("Bluetooth device not found");

    // Connect to the device
    peripheral.connect().await.unwrap();
    peripheral.discover_services().await.unwrap();
    println!("Connected to Bluetooth device.");

    // // Setup Modbus client

    for c in peripheral.characteristics().into_iter() {
        let uuid = c.uuid;
        let props = c.properties;
        println!("{uuid} {props:?}");

        if props.contains(CharPropFlags::READ) {
            let read_result = peripheral.read(&c).await;
            match read_result {
                Ok(data) => println!("{uuid} = {data:?}"),
                Err(e) => println!("{uuid} error: {e}"),
            }
        }
    }

    // -----

    let mut socket = BtSocket::new(BtProtocol::RFCOMM).unwrap();
    socket.connect(BtAddr(peripheral.address().into_inner())).unwrap();

    // 00001534-1212-efde-1523-785feabcd123 READ : Device Firmware Version
    // 00002a26-0000-1000-8000-00805f9b34fb READ : Firmware Revision String
    // 00002a29-0000-1000-8000-00805f9b34fb READ : Manufacturer Name String

    // tokio_modbus::prelude::tcp::connect(peripheral);
    // let socket_addr = "192.168.0.100:502"; // Replace with your Modbus server address
    // let client = tcp::connect(socket_addr).await.unwrap();
    // let client = Arc::new(client);

    // // Read Modbus data in a loop
    // loop {
    //     let read = client.read_holding_registers(0x00, 10).await.unwrap();
    //     println!("Read data: {:?}", read);

    //     sleep(Duration::from_secs(5)).await;
    // }
}

// async fn print_bms_details() {
//     // Initialize the Bluetooth manager
//     let manager = Manager::new().await.unwrap();

//     // Get the first Bluetooth adapter
//     let adapters = manager.adapters().await.unwrap();
//     let central = adapters.into_iter().nth(0).expect("No Bluetooth adapter found");

//     central.start_scan(ScanFilter::default()).await.unwrap();

//     // Get the BMS peripheral
//     let peripherals = central.peripherals().await.unwrap();

//     for p in peripherals.iter() {
//         let props = p.properties().await.unwrap();
//         if let Some(props) = props {
//             println!("{props:?}");
//         }
//     }

//     let peripheral = find_peripheral(peripherals, DEVICE_NAME).await.expect("Bluetooth device not found");

//     // Connect to the device
//     peripheral.connect().await.unwrap();
//     peripheral.discover_services().await.unwrap();

//     // Print characteristics and their values
//     for c in peripheral.characteristics().into_iter() {
//         let uuid = c.uuid;
//         let props = c.properties;
//         print!("{uuid} {props:?} ");

//         if props.contains(CharPropFlags::READ) {
//             let read_result = peripheral.read(&c).await;
//             match read_result {
//                 Ok(data) => println!("= {data:?}"),
//                 Err(e) => println!("error: {e}"),
//             }
//         } else {
//             println!("")
//         }
//     }

async fn find_peripheral<T>(peripherals: Vec<T>, device_name: &'static str) -> Option<T> where T: Peripheral {
    for p in peripherals.into_iter() {
        let local_name = p.properties().await.ok().flatten().map(|p| p.local_name).flatten();
        match local_name {
            Some(name) if name == device_name => return Some(p),
            _ => {}
        }
    }
    return None
}

