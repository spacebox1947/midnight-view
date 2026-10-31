#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_println as _;
use embassy_futures::select::{select3, Either3};

use esp_println as _;
use defmt::info;
use defmt::error;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]

// ---- ---- ---- [events] ---- ---- ----
#[derive(PartialEq, Clone, Copy, Debug, defmt::Format)]
enum ButtonEvent {
    Pressed,
    Cleared
}

static BUTTON_EVENT_LEFT: Signal<CriticalSectionRawMutex, ButtonEvent> = Signal::new();
static BUTTON_EVENT_RIGHT: Signal<CriticalSectionRawMutex, ButtonEvent> = Signal::new();
// ---- ---- ---- [main] ---- ---- ----
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32s3 -o unstable-hal -o alloc -o wifi -o embassy -o defmt -o neovim

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    // ---- ---- ---- [Wifi Setup] ---- ---- ----
    let (mut _wifi_controller, _interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    // ---- ---- ---- [InputConfiguration] ---- ---- ----
    let encoder_config = InputConfig::default().with_pull(Pull::Up);
    let button_config = InputConfig::default().with_pull(Pull::None);
    let button_left_input = Input::new(peripherals.GPIO7, button_config);
    let button_right_input = Input::new(peripherals.GPIO9, button_config);
    let encoder_a = Input::new(peripherals.GPIO4, encoder_config);
    let encoder_b = Input::new(peripherals.GPIO5, encoder_config);
    let encoder_x = Input::new(peripherals.GPIO6, encoder_config);
    
    // ---- ---- ---- [OutputConfiguration] ---- ---- ----
    let led_config = OutputConfig::default().with_pull(Pull::Up);
    let mut led_left = Output::new(peripherals.GPIO15, Level::Low, led_config);
    let mut led_right = Output::new(peripherals.GPIO16, Level::Low, led_config);

    // ---- ---- ---- [Spawner] ---- ---- ----
    spawner.spawn(encoder_event(encoder_a,encoder_b,encoder_x).unwrap());
    spawner.spawn(button_event(button_left_input, &BUTTON_EVENT_LEFT).unwrap());
    spawner.spawn(button_event(button_right_input, &BUTTON_EVENT_RIGHT).unwrap());

    loop {
        match select3(BUTTON_EVENT_LEFT.wait(), BUTTON_EVENT_RIGHT.wait(), Timer::after_secs(5)).await {
            Either3::First(ev) => {
                info!("Got Left Button Event: {}", ev);
            },
            Either3::Second(ev) => {
                info!("Got Right Button Event: {}", ev);
            },
            Either3::Third(_) => {
                info!("No button Event after 5 seconds");
            },
        }
    };
}

#[embassy_executor::task(pool_size = 2)]
async fn button_event(mut button: Input<'static>, event: &'static Signal<CriticalSectionRawMutex, ButtonEvent>) {
    loop {
        button.wait_for_low().await;
        event.signal(ButtonEvent::Pressed);
        Timer::after(Duration::from_millis(20)).await;
    }
}

#[embassy_executor::task]
async fn encoder_event(mut encoder_a: Input<'static>, encoder_b: Input<'static>, encoder_x: Input<'static>) {
    let mut delta: f32 = 0.0;
    loop {
        encoder_a.wait_for_any_edge().await;

        let a = encoder_a.is_high();
        let b = encoder_b.is_high();
        let x = encoder_x.is_high();

        delta += if a == b { -0.01 } else { 0.01 };

        info!("Delta Changed!: {}", delta);
        if x {
            info!("Encoder Index Triggered!: {}", x);
        }
        Timer::after_millis(500).await;
    }
}
