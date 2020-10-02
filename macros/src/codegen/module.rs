use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rtic_syntax::{ast::App, Context};

use crate::{check::Extra, codegen::util};

pub fn codegen(ctxt: Context, resources_tick: bool, app: &App, extra: &Extra) -> TokenStream2 {
    let mut items = vec![];
    let mut fields = vec![];
    let mut values = vec![];

    let name = ctxt.ident(app);

    let mut needs_instant = false;
    let mut lt = None;
    match ctxt {
        Context::Init => {
            if app.uses_schedule() {
                let m = extra.monotonic();

                fields.push(quote!(
                    /// System start time = `Instant(0 /* cycles */)`
                    pub start: <#m as rtic::Monotonic>::Instant
                ));

                values.push(quote!(start: <#m as rtic::Monotonic>::zero()));

                fields.push(quote!(
                    /// Core (Cortex-M) peripherals minus the SysTick
                    pub core: rtic::Peripherals
                ));
            } else {
                fields.push(quote!(
                    /// Core (Cortex-M) peripherals
                    pub core: rtic::export::Peripherals
                ));
            }

            if extra.peripherals {
                let device = extra.device;

                fields.push(quote!(
                    /// Device peripherals
                    pub device: #device::Peripherals
                ));

                values.push(quote!(device: #device::Peripherals::steal()));
            }

            lt = Some(quote!('a));
            fields.push(quote!(
                /// Critical section token for init
                pub cs: rtic::export::CriticalSection<#lt>
            ));

            values.push(quote!(cs: rtic::export::CriticalSection::new()));

            values.push(quote!(core));
        }

        Context::Idle => {}

        Context::HardwareTask(..) => {
            if app.uses_schedule() {
                let m = extra.monotonic();

                fields.push(quote!(
                    /// Time at which this handler started executing
                    pub start: <#m as rtic::Monotonic>::Instant
                ));

                values.push(quote!(start: instant));

                needs_instant = true;
            }
        }

        Context::SoftwareTask(..) => {
            if app.uses_schedule() {
                let m = extra.monotonic();

                fields.push(quote!(
                    /// The time at which this task was scheduled to run
                    pub scheduled: <#m as rtic::Monotonic>::Instant
                ));

                values.push(quote!(scheduled: instant));

                needs_instant = true;
            }
        }
    }

    if ctxt.has_locals(app) {
        let ident = util::locals_ident(ctxt, app);
        items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Locals;
        ));
    }

    if ctxt.has_resources(app) {
        let ident = util::resources_ident(ctxt, app);
        let lt = if resources_tick {
            lt = Some(quote!('a));
            Some(quote!('a))
        } else {
            None
        };

        items.push(quote!(
            #[doc(inline)]
            pub use super::#ident as Resources;
        ));

        fields.push(quote!(
            /// Resources this task has access to
            pub resources: Resources<#lt>
        ));

        let priority = if ctxt.is_init() {
            None
        } else {
            Some(quote!(priority))
        };
        values.push(quote!(resources: Resources::new(#priority)));
    }

    if ctxt.uses_schedule(app) {
        let doc = "Tasks that can be `schedule`-d from this context";
        if ctxt.is_init() {
            items.push(quote!(
                #[doc = #doc]
                #[derive(Clone, Copy)]
                pub struct Schedule {
                    _not_send: core::marker::PhantomData<*mut ()>,
                }
            ));

            fields.push(quote!(
                #[doc = #doc]
                pub schedule: Schedule
            ));

            values.push(quote!(
                schedule: Schedule { _not_send: core::marker::PhantomData }
            ));
        } else {
            lt = Some(quote!('a));

            items.push(quote!(
                #[doc = #doc]
                #[derive(Clone, Copy)]
                pub struct Schedule<'a> {
                    priority: &'a rtic::export::Priority,
                }

                impl<'a> Schedule<'a> {
                    #[doc(hidden)]
                    #[inline(always)]
                    pub unsafe fn priority(&self) -> &rtic::export::Priority {
                        &self.priority
                    }
                }
            ));

            fields.push(quote!(
                #[doc = #doc]
                pub schedule: Schedule<'a>
            ));

            values.push(quote!(
                schedule: Schedule { priority }
            ));
        }
    }

    if ctxt.uses_spawn(app) {
        let doc = "Tasks that can be `spawn`-ed from this context";
        if ctxt.is_init() {
            fields.push(quote!(
                #[doc = #doc]
                pub spawn: Spawn
            ));

            items.push(quote!(
                #[doc = #doc]
                #[derive(Clone, Copy)]
                pub struct Spawn {
                    _not_send: core::marker::PhantomData<*mut ()>,
                }
            ));

            values.push(quote!(spawn: Spawn { _not_send: core::marker::PhantomData }));
        } else {
            lt = Some(quote!('a));

            fields.push(quote!(
                #[doc = #doc]
                pub spawn: Spawn<'a>
            ));

            let mut instant_method = None;
            if ctxt.is_idle() {
                items.push(quote!(
                    #[doc = #doc]
                    #[derive(Clone, Copy)]
                    pub struct Spawn<'a> {
                        priority: &'a rtic::export::Priority,
                    }
                ));

                values.push(quote!(spawn: Spawn { priority }));
            } else {
                let instant_field = if app.uses_schedule() {
                    let m = extra.monotonic();

                    needs_instant = true;
                    instant_method = Some(quote!(
                        pub unsafe fn instant(&self) -> <#m as rtic::Monotonic>::Instant {
                            self.instant
                        }
                    ));
                    Some(quote!(instant: <#m as rtic::Monotonic>::Instant,))
                } else {
                    None
                };

                items.push(quote!(
                    /// Tasks that can be spawned from this context
                    #[derive(Clone, Copy)]
                    pub struct Spawn<'a> {
                        #instant_field
                        priority: &'a rtic::export::Priority,
                    }
                ));

                let _instant = if needs_instant {
                    Some(quote!(, instant))
                } else {
                    None
                };
                values.push(quote!(
                    spawn: Spawn { priority #_instant }
                ));
            }

            items.push(quote!(
                impl<'a> Spawn<'a> {
                    #[doc(hidden)]
                    #[inline(always)]
                    pub unsafe fn priority(&self) -> &rtic::export::Priority {
                        self.priority
                    }

                    #instant_method
                }
            ));
        }
    }

    if let Context::Init = ctxt {
        let init = &app.inits.first().unwrap();
        if init.returns_late_resources {
            let late_resources = util::late_resources_ident(&init.name);

            items.push(quote!(
                #[doc(inline)]
                pub use super::#late_resources as LateResources;
            ));
        }
    }

    let doc = match ctxt {
        Context::Idle => "Idle loop",
        Context::Init => "Initialization function",
        Context::HardwareTask(_) => "Hardware task",
        Context::SoftwareTask(_) => "Software task",
    };

    let core = if ctxt.is_init() {
        if app.uses_schedule() {
            Some(quote!(core: rtic::Peripherals,))
        } else {
            Some(quote!(core: rtic::export::Peripherals,))
        }
    } else {
        None
    };

    let priority = if ctxt.is_init() {
        None
    } else {
        Some(quote!(priority: &#lt rtic::export::Priority))
    };

    let instant = if needs_instant {
        let m = extra.monotonic();

        Some(quote!(, instant: <#m as rtic::Monotonic>::Instant))
    } else {
        None
    };

    items.push(quote!(
        /// Execution context
        pub struct Context<#lt> {
            #(#fields,)*
        }

        impl<#lt> Context<#lt> {
            #[inline(always)]
            pub unsafe fn new(#core #priority #instant) -> Self {
                Context {
                    #(#values,)*
                }
            }
        }
    ));

    if !items.is_empty() {
        quote!(
            #[allow(non_snake_case)]
            #[doc = #doc]
            pub mod #name {
                #(#items)*
            }
        )
    } else {
        quote!()
    }
}
