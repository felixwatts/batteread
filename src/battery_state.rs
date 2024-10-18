/// The reported state of the battery
#[derive(Debug)]
pub struct BatteryState {
    /// The state of charge of the battery in %
    pub state_of_charge_pct: u16,
    /// The residual capacity of the battery in Ah/100
    pub residual_capacity_cah: u16,
    /// Lifetime number of battery cycles (count)
    pub cycles_count: u16,
    /// The voltage of each cell in mv. The N/A value is 61001
    pub cell_voltage_mv: Vec<u16>,
    /// The battery voltage in V/100
    pub battery_voltage_cv: u16,
}