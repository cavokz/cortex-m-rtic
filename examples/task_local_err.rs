//! examples/task_local_err.rs
#![deny(unsafe_code)]
#![deny(warnings)]
#![no_main]
#![no_std]

use cortex_m_semihosting::{debug, hprintln};
use panic_semihosting as _;

#[rtic::app(device = lm3s6965)]
const APP: () = {
    struct Resources {
        // A local (move), late resource
        #[task_local]
        l: u32,
    }

    #[init]
    fn init(_: init::Context) -> init::LateResources {
        init::LateResources { l: 42 }
    }

    // l is task_local
    #[idle(resources =[l])]
    fn idle(cx: idle::Context) -> ! {
        hprintln!("IDLE:l = {}", cx.resources.l).unwrap();
        debug::exit(debug::EXIT_SUCCESS);
        loop {}
    }

    // l error, (task_local used twice)
    #[task(priority = 1, binds = UART1, resources = [l])]
    fn uart1(_cx: uart1::Context) {}
};
