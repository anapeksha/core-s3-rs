#![no_std]
#![no_main]

use core::cell::RefCell;

use critical_section::Mutex;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    system::{CpuControl, Stack},
    time::Duration,
};
use static_cell::StaticCell;

static APP_CORE_STACK: StaticCell<Stack<8192>> = StaticCell::new();
static APP_CORE_TICKS: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(0));

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let app_core_stack = APP_CORE_STACK.init(Stack::new());
    let _app_core_guard = cpu_control
        .start_app_core(app_core_stack, app_core_task)
        .expect("start ESP32-S3 app core");

    loop {
        delay.delay(Duration::from_secs(1));
        let ticks = critical_section::with(|cs| *APP_CORE_TICKS.borrow_ref(cs));
        esp_println::println!("PRO CPU alive; APP CPU ticks={ticks}");
    }
}

fn app_core_task() {
    let delay = Delay::new();

    loop {
        delay.delay(Duration::from_millis(250));
        critical_section::with(|cs| {
            let mut ticks = APP_CORE_TICKS.borrow_ref_mut(cs);
            *ticks = ticks.wrapping_add(1);
        });
    }
}
