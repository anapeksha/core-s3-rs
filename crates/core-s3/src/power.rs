/// Battery/PMU level abstractions shared by examples and future AXP2101 driver glue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeState {
    Unknown,
    Discharging,
    Charging,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryStatus {
    pub millivolts: u16,
    pub state: ChargeState,
}

impl BatteryStatus {
    pub const fn new(millivolts: u16, state: ChargeState) -> Self {
        Self { millivolts, state }
    }
}
