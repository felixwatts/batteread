use std::pin::Pin;
use std::task::{Context, Poll};
use btleplug::api::{Central, CharPropFlags, Characteristic, Peripheral, ScanFilter, WriteType};
use btleplug::api::Manager as _;
use btleplug::platform::Manager;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::futures;
use tokio_modbus::prelude::*;
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use futures_util::{FutureExt, StreamExt};

fn NORDIC_UART_WRITE_CHARACTERISTIC() -> Characteristic {
    Characteristic {
        uuid: Uuid::parse_str("6e400002-b5a3-f393-e0a9-e50e24dcca9e").unwrap(),
        service_uuid: Uuid::parse_str("6e400001-b5a3-f393-e0a9-e50e24dcca9e").unwrap(),
        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::WRITE,
        descriptors: Default::default()
    }
}

fn NORDIC_UART_READ_CHARACTERISTIC() -> Characteristic {
    Characteristic{
        uuid: Uuid::parse_str("6e400003-b5a3-f393-e0a9-e50e24dcca9e").unwrap(),
        service_uuid: Uuid::parse_str("6e400001-b5a3-f393-e0a9-e50e24dcca9e").unwrap(),
        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::WRITE,
        descriptors: Default::default()
    }
}

#[tokio::main]
async fn main() {
    // Initialize the Bluetooth manager
    let manager = Manager::new().await.unwrap();

    // Get the first Bluetooth adapter
    let adapters = manager.adapters().await.unwrap();
    let central = adapters.into_iter().nth(0).expect("No Bluetooth adapter found");

    // Start scanning for devices
    central.start_scan(ScanFilter::default()).await.unwrap();

    println!("Scanning for 30s");

    sleep(Duration::from_secs(30)).await; // Allow some time to discover devices

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

    println!("SERVICES\n");

    let services = peripheral.services();
    for s in services.iter() {
        println!("{s:?}");
        for c in s.characteristics.iter() {
            println!("    {c:?}");
        }
    }

    println!("CHARACTERISTICS\n");

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

    // Services

    // 00001530-1212-efde-1523-785feabcd123 : Nordic Device Firmware Update Service
        // 00001531-1212-efde-1523-785feabcd123 WRITE | NOTIFY : Device firmware update
        // 00001532-1212-efde-1523-785feabcd123 WRITE_WITHOUT_RESPONSE : Frimware packet
        // 00001534-1212-efde-1523-785feabcd123 READ : Device Firmware Version = 1 0

    // 0000180a-0000-1000-8000-00805f9b34fb : GATT device information
        // 00002a26-0000-1000-8000-00805f9b34fb READ : Firmware Revision String = B0163,V1.05
        // 00002a29-0000-1000-8000-00805f9b34fb READ : Manufacturer Name String = SKYLAB

    // 6e400001-b5a3-f393-e0a9-e50e24dcca9e : Nordic UART Service
        // 6e400002-b5a3-f393-e0a9-e50e24dcca9e WRITE_WITHOUT_RESPONSE | WRITE : UART write
        // 6e400003-b5a3-f393-e0a9-e50e24dcca9e NOTIFY : UART read


    println!("Try read Nordic UART");

    let result = peripheral.read(&NORDIC_UART_READ_CHARACTERISTIC()).await;

    println!("{result:?}");

    println!("Try subscribe Nordic UART");

    let result = peripheral.subscribe(&NORDIC_UART_READ_CHARACTERISTIC()).await;

    println!("{result:?}");

    let p2 = peripheral.clone();

    let p3 = peripheral.clone();
    let join_handle = tokio::spawn(async move {
        println!("Waiting for notifications...");
        loop {
            let result = p3.notifications().await;

            println!("Received notifcation");

            match result {
                Ok(stream) => {
                    stream.for_each(|i| async move { println!("{i:?}") }).await;
                },
                Err(e) => {
                    println!("{e:?}");
                    break;
                }
            }
        }
    });

    // Try to set up modbus stream
    let nordic_uart_stream = NordicUartStream::new(p2);
    let mut modbus = tokio_modbus::prelude::rtu::attach(nordic_uart_stream);

    println!("send modbus request");
    let result = modbus.read_holding_registers(0x0, 1).await;
    println!("modbus result: {result:?}");
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

#[derive(Debug)]
struct NordicUartStream<T> where T: Peripheral {
    peripheral: T,
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
}

impl<T> NordicUartStream<T> where T: Peripheral {
    fn new(peripheral: T) -> Self {
        Self {
            peripheral,
            read_buffer: Vec::new(),
            write_buffer: Vec::new(),
        }
    }
}

impl<T> AsyncRead for NordicUartStream<T> where T: Peripheral + Unpin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<tokio::io::Result<()>> {
        let peripheral = &mut self.peripheral;
        let fut = async move {
            let result = peripheral.read(&NORDIC_UART_READ_CHARACTERISTIC()).await;
            result
        };
        
        let data:Vec<u8> = std::task::ready!(Box::pin(fut).poll_unpin(cx)).map_err(|e| tokio::io::Error::new(std::io::ErrorKind::BrokenPipe, e))?;
        buf.put_slice(&data);
        Poll::Ready(Ok(()))
    }
}

impl<T> AsyncWrite for NordicUartStream<T> where T: Peripheral + Unpin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<tokio::io::Result<usize>> {
        self.write_buffer.extend_from_slice(buf);
        let data = self.write_buffer.clone();
        let peripheral = &mut self.peripheral;
        
        let fut = async move {
            peripheral.write(&NORDIC_UART_WRITE_CHARACTERISTIC(), &data, WriteType::WithResponse).await.map(|_| data.len())
        };

        let n = std::task::ready!(Box::pin(fut).poll_unpin(cx)).map_err(|e| tokio::io::Error::new(std::io::ErrorKind::BrokenPipe, e))?;
        self.write_buffer.clear();
        Poll::Ready(Ok(n))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<tokio::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<tokio::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}