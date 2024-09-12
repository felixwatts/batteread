mod battery_client;

pub use battery_client::BatteryClient;
pub use battery_client::BatteryState;

// #[tokio::main]
// async fn main() {
    // let mut client = BatteryClient::new().await.unwrap();

    // let battery_state = client.fetch_state().await;

    // println!("{battery_state:?}");

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

    // Protocol reverse engineering

    // Client sends

    // Server replies with 3 notifications making up 1 reply

    // -> 01 03 32 02 44 00 00 00 00 00 00 00 00 00 00 02 44 00 00
    // -> 00 00 00 02 58 02 44 02 44 00 00 00 10 00 61 00 64 4b 61 4e 20
    // -> 4e 20 00 23 00 00 00 00 00 00 00 00 00 00 ba b1

    // Payload as 16 bit ints:
    //
    // 17408 0 0 0 0 2 17408 0 0 600 580 580 0 16 97 100 19297 20000 20000 35 0 0 0 0 0

    // seems to mean
    // 
    // 01 03 -> message type (same as request)
    // 32    -> Payload length (50) (includes checksum bytes)
    // ...   -> Payload
    // ba b1 -> Checksum (2 bytes)
    //
    // Checksum
    // 
    // its CRC MODBUS of the whole message including the message type, length and payload, and the two bytes of the CRC are reversed.
    //
    // Payload
    // Byte range | meaning
    // 1          | ? "2"
    // 2-3        | ? "17408" or "68 0" temp in F?
    // 4-29       | ?
    // 31-32      | State of charge %
    // 41-42      | # cycles
    // 
    // 02 
    // 44 00 00 00 
    // 00 00 00 00 
    // 00 00 00 02 
    // 44 00 00 00 
    // 00 00 02 58 
    // 02 44 02 44 
    // 00 00 00 10 
    // 00 61 00 64 
    // 4b 61 4e 20 
    // 4e 20 00 23 
    // 00 00 00 00 
    // 00 00 00 00 
    // 00 00
    // 
    // Seems to be 4 byte numbers
    // 
    // Guess:
    //
    // byte at position 30 [29] and/or 34 [33] is State of Charge Percent
    // byte at position 41 is # cycles

    // REQUEST (Voltages)
    //
    // 0103d0000026fcd0
    //
    // RESPONSE
    //
    // Cell voltages: 32 * 16 bit unsigned int in mV
    // Vol range min+max: 2 * 16 bit unsigned int in mV
    // 6 bytes: ??
    // Battery volatage: 1 * 16 bit unsigned in in mV
    //
    // Example payload
    //
    // 0d030d040d050d000d030d050d040cffee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee490d050cff0003000800060a682771

    // REQUEST (State of Charge)
    //
    // 0103d02600195d0b
    //
    // RESPONSE
    //
    // 16 bit unsigned ints
    //
    // 14: State of charge %
    // 16: Residual capacity mAh
    // 19: Cycles #
    //
    // bytes 31-32 and/or 34: State of Charge %
    // bytes 36-37: Temp. in C/1000 or Residual capacity in mAh/10
    // bytes 41-42: # Cycles
    //
    // Example payload
    //
    // 02 44 00 00 
    // 00 00 00 00 
    // 00 00 00 00 
    // 02 44 00 00 
    // 00 00 00 02 
    // 58 02 44 02 
    // 44 00 00 00 
    // 10 00 61 00 
    // 64 4b 61 4e 
    // 20 4e 20 00 
    // 23 00 00 00 
    // 00 00 00 00 
    // 00 00 00
    //
    // Another example
    //
    // 02 44 00 00 
    // 00 00 00 00 
    // 00 00 00 00
    // 02 44 00 00 
    // 00 00 02 58
    // 02 44 02 44
    // 00 00 00 10
    // 00 61 00 64
    // 4b 61 4e 20
    // 4e 20 00 23
    // 00 00 00 00
    // 00 00 00 00
    // 00 00



    // REQUEST
    //
    // 0103d1000015bd39
    //
    // RESPONSE
    //
    // byte 35 : Current in A/100 ??? 
    //
    // Example Payload
    //
    // 0000000000000000000010101010221b1b1b000000000000000000000000000000009600000000000000

    // 0103d115000c6d37 -> 010318 24 0c 00 00 02 a7 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
                //      -> 010318 240c000002a7000000000000000000000000000000000000
                //      -> 010318 240c000002a7000000000000000000000000000000000000
    // 0103d1000015bd39 -> 0000000000000000000010101010221b1b1b000000000000000000000000000000009600000000000000 seems to be same reply every time
                //      -> 0000000000000000000010101010221b1b1b000000000000000000000000000000009600000000000000
                        
    // 0103d0000026fcd0 -> 0d030d040d050d000d030d060d040cffee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee490d060cff000600080007
    //                  -> 0cfc0cfc0cfd0cf90cfc0cfd0cfd0cf8ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee49ee490204201b00170004001b1d00ee49ee49ee49ee490cfd0cf80003000800050a62
    // 0103d02600195d0b -> 024400000000000000000000024e000000000  258024e02440000000f006100644b604e204e20002300000000000000000000
                        // 0244000000000000000000000244000000000002580244024400000010006100644b614e204e20002300000000000000000000
                        // 0244000000000000000000000244000000000  2580244024400000010006100644b614e204e20002300000000000000000000
                        // 0244000000000000000000000244000000000  24e0244024400000006005b0064467e4e204e20002400000000000000000000 9249

                        


                        // 0244 0000 0000 0000 0000 0000
                        // 024e 0000 0000
                        // 0258
                        // 024e
                        // 0244 0000 000f 0061 0064 4b60
                        // 4e20
                        // 4e20
                        // 0023 0000 0000 0000 0000 0000

                        // 0244 0000 0000 0000 0000 0000
                        // 0244 0000 0000
                        // 0258
                        // 0244
                        // 0244 0000 0010 0061 0064 4b61 
                        // 4e20 4e20 
                        // 0023 0000 0000 0000 0000 0000




// }
