/// A verbatim message to send which requests the state of change and related data
pub (crate) const REQUEST: [u8; 8] = [0x01, 0x03, 0xd0, 0x26, 0x00, 0x19, 0x5d, 0x0b];

/// A message type which contains data about state of charge and battery condition
pub (crate) struct SocMessage(Vec<u16>);

impl SocMessage{
    pub fn new(data: Vec<u8>) -> Self{
        let nums: Vec<u16> = data
            .chunks(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect();
        println!("BATTERY SOC response: {nums:?}");
        Self(nums)
    }

    pub fn state_of_charge_pct(&self) -> u16 {
        self.0[14]
    }

    pub fn residual_capacity_cah(&self) -> u16 {
        self.0[16]
    }

    pub fn cycles_count(&self) -> u16 {
        self.0[19]
    }
}

