/// A verbatim message to send which requests state of voltages
pub (crate) const REQUEST: [u8; 8] = [0x01, 0x03, 0xd0, 0x00, 0x00, 0x26, 0xfc, 0xd0];

const CELL_VOLTAGE_NA_VALUE: u16 = 61001;

/// A message type which contains data about battery and cell voltages.
pub (crate) struct VoltagesMessage(Vec<u16>);

impl VoltagesMessage{
    pub fn new(data: Vec<u8>) -> Self{
        let nums: Vec<u16> = data
            .chunks(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect();
        println!("BATTERY Voltages response: {nums:?}");
        Self(nums)
    }

    pub fn cell_voltage_mv(&self) -> Vec<u16> {
        self.0[0..32].iter().cloned().filter(|&v| v != CELL_VOLTAGE_NA_VALUE).collect()
    }

    pub fn battery_voltage_cv(&self) -> u16 {
        self.0[37]
    }
}

