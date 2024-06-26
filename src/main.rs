// use bluetooth_serial_port::{BtProtocol, BtSocket};
// use std::io::{Read, Write};
// use std::time;
// use mio::{Poll, Token, Interest};

// fn main() {
    // scan for devices
    // let devices = bluetooth_serial_port::scan_devices(time::Duration::from_secs(20)).unwrap();
    // if devices.len() == 0 {
    //     panic!("No devices found");
    // }

    // println!("Found bluetooth devices {:?}", devices);

    // // "device.addr" is the MAC address of the device
    // let device = devices.into_iter().find(|d| d.name == "BT_HC6172").expect("Device not found");
    // println!(
    //     "Connecting to `{}` ({})",
    //     device.name,
    //     device.addr.to_string()
    // );

    // // create and connect the RFCOMM socket
    // let mut socket = BtSocket::new(BtProtocol::RFCOMM).unwrap();
    // socket.connect(device.addr).unwrap();

    // // BtSocket implements the `Read` and `Write` traits (they're blocking)
    // let mut buffer = [0; 10];
    // let num_bytes_read = socket.read(&mut buffer[..]).unwrap();
    // let num_bytes_written = socket.write(&buffer[0..num_bytes_read]).unwrap();
    // println!(
    //     "Read `{}` bytes, wrote `{}` bytes",
    //     num_bytes_read, num_bytes_written
    // );

    // // BtSocket also implements `mio::Evented` for async IO
    // let poll = Poll::new().unwrap();
    // poll.registry().register(&mut socket, Token(0), Interest::READABLE | Interest::WRITABLE).unwrap();
    // // loop { ... poll events and wait for socket to be readable/writable ... }
// }

use btleplug::api::{Central, CharPropFlags, Peripheral, ScanFilter};
use btleplug::api::Manager as _;
use btleplug::platform::Manager;
use tokio_modbus::prelude::*;
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
    sleep(Duration::from_secs(15)).await; // Allow some time to discover devices

    // Find the specified device by name
    let device_name = "BT_HC6172";
    let peripherals = central.peripherals().await.unwrap();

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

    // 00001534-1212-efde-1523-785feabcd123 READ
    // 00002a26-0000-1000-8000-00805f9b34fb READ
    // 00002a29-0000-1000-8000-00805f9b34fb READ

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
