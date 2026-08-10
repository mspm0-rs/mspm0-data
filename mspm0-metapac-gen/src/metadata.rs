use std::{collections::HashSet, sync::LazyLock};

use mspm0_data_types::{
    AdcInternalSource, Chip, MemoryKind, Package, Peripheral, PeripheralType, PowerDomain,
    PowerMode,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use regex::Regex;

static GPIO_PIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^P(?<bank>[A-Z])\d+").unwrap());

pub fn pins(chip: &Chip, package: &Package) -> TokenStream {
    // Filter for pins available on this package.
    let pins = package.pins.iter().filter_map(|pin| {
        // We need to match explicitly for GPIO pins.
        //
        // The metadata contains both non-GPIO pins (VCORE, VSS, VDD) and NRST.
        // On parts where a GPIO pin and NRST are bonded, we need to pick the GPIO pins.
        let signal = pin
            .signals
            .iter()
            .find(|signal| GPIO_PIN.is_match(signal))?;

        let pincm = chip
            .iomux
            .get(signal)
            .expect("Signal did not have an iomux pincm entry");
        let pincm = Literal::u8_suffixed(*pincm as u8);
        let wakeup = match &chip.wakeup_pins {
            Some(wakeup_pins) => {
                let wakeup = wakeup_pins.contains(signal);
                quote! { Some(#wakeup) }
            }
            None => quote! { None },
        };

        Some(quote! { Pin { pin: #signal, pincm: #pincm, wakeup: #wakeup } })
    });

    quote! { &[#(#pins),*] }
}

pub fn memory(chip: &Chip) -> TokenStream {
    let regions = chip.memory.iter().map(|region| {
        let name = &region.name;
        let kind = match region.kind {
            MemoryKind::Flash => quote! { MemoryKind::Flash },
            MemoryKind::Ram => quote! { MemoryKind::Ram },
        };
        let address = Literal::u32_unsuffixed(region.address);
        let size = Literal::u32_unsuffixed(region.length * 1024); // Convert from KB to B
        let retained_through = power_mode(region.retained_through);

        quote! {
            MemoryRegion {
                name: #name,
                kind: #kind,
                address: #address,
                size: #size,
                retained_through: #retained_through,
            }
        }
    });

    quote! { &[#(#regions),*] }
}

pub fn peripherals(chip: &Chip, package: &Package) -> TokenStream {
    // Peripheral pins should only be marked as available if the package contains the pin.
    //
    // So we make a list of pins for quick lookup.
    let pins = package
        .pins
        .iter()
        .filter_map(|pin| pin.signals.iter().find(|signal| GPIO_PIN.is_match(signal)))
        .cloned()
        .collect::<HashSet<String>>();

    let mut peripherals = Vec::<TokenStream>::new();

    for peri in chip.peripherals.values() {
        if let Some(peri) = generate_peripheral(peri, &pins, package) {
            peripherals.push(peri);
        }
    }

    quote! { &[#(#peripherals),*] }
}

pub fn dma_channels(chip: &Chip) -> TokenStream {
    let mut dma_channels = Vec::new();

    for (&num, channel) in chip.dma_channels.iter() {
        let number = Literal::u32_unsuffixed(num);
        let full = channel.full;

        dma_channels.push(quote! {
            DmaChannel {
                number: #number,
                full: #full,
            }
        });
    }

    quote! {
        &[#(#dma_channels),*]
    }
}

pub fn interrupts(chip: &Chip) -> TokenStream {
    let mut interrupts = Vec::new();

    for interrupt in chip.interrupts.values() {
        // Skip interrupts handled by cortex-m
        if interrupt.num < 0 {
            continue;
        }

        let number = Literal::i32_unsuffixed(interrupt.num);
        let name = &interrupt.name;

        interrupts.push(quote! {
            Interrupt {
                name: #name,
                number: #number,
            }
        });
    }

    quote! {
        &[#(#interrupts),*]
    }
}

pub fn interrupt_groups(chip: &Chip) -> TokenStream {
    let mut groups = Vec::new();

    for interrupt in chip.interrupts.values() {
        // Skip interrupts handled by cortex-m
        if interrupt.num < 0 {
            continue;
        }

        if interrupt.group.is_empty() {
            continue;
        }

        let mut entries = Vec::new();

        for (index, interrupt) in interrupt.group.iter() {
            let number = Literal::u32_unsuffixed(*index);

            entries.push(quote! {
                GroupInterrupt {
                    name: #interrupt,
                    number: #number
                }
            });
        }

        let name = &interrupt.name;
        let number = Literal::u32_unsuffixed(interrupt.num as u32);

        groups.push(quote! {
            InterruptGroup {
                name: #name,
                number: #number,
                interrupts: &[#(#entries),*]
            }
        });
    }

    quote! {
        &[#(#groups),*]
    }
}

fn power_mode(mode: PowerMode) -> TokenStream {
    match mode {
        PowerMode::Run => quote! { PowerMode::Run },
        PowerMode::Sleep => quote! { PowerMode::Sleep },
        PowerMode::Stop0 => quote! { PowerMode::Stop0 },
        PowerMode::Stop1 => quote! { PowerMode::Stop1 },
        PowerMode::Stop2 => quote! { PowerMode::Stop2 },
        PowerMode::Standby0 => quote! { PowerMode::Standby0 },
        PowerMode::Standby1 => quote! { PowerMode::Standby1 },
        PowerMode::Shutdown => quote! { PowerMode::Shutdown },
    }
}

fn adc_internal_source(source: AdcInternalSource) -> TokenStream {
    match source {
        AdcInternalSource::TemperatureSensor => quote! { AdcInternalSource::TemperatureSensor },
        AdcInternalSource::Opa0 => quote! { AdcInternalSource::Opa0 },
        AdcInternalSource::Opa1 => quote! { AdcInternalSource::Opa1 },
        AdcInternalSource::Gpamp => quote! { AdcInternalSource::Gpamp },
        AdcInternalSource::Dac0 => quote! { AdcInternalSource::Dac0 },
        AdcInternalSource::Vref => quote! { AdcInternalSource::Vref },
        AdcInternalSource::SupplyMonitor => quote! { AdcInternalSource::SupplyMonitor },
        AdcInternalSource::VbatMonitor => quote! { AdcInternalSource::VbatMonitor },
        AdcInternalSource::VusbMonitor => quote! { AdcInternalSource::VusbMonitor },
    }
}

fn skip_peripheral(ty: PeripheralType) -> bool {
    matches!(ty, PeripheralType::Unknown)
}

fn generate_peripheral(
    peripheral: &Peripheral,
    available_pins: &HashSet<String>,
    package: &Package,
) -> Option<TokenStream> {
    // Exclude peripherals that don't really exist as singletons.
    if skip_peripheral(peripheral.ty) {
        return None;
    }

    let name = &peripheral.name;
    let kind = &peripheral.ty.to_string();
    let version = match &peripheral.version {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };

    let mut pins = Vec::<TokenStream>::new();

    for pin in peripheral.pins.iter() {
        let name = &pin.pin;
        let signal = &pin.signal;
        let pf = match pin.pf {
            Some(pf) => quote! { Some(#pf) },
            None => quote! { None },
        };

        if available_pins.contains(name) || name == "NRST" {
            // If NRST is being used, figure out what pin it truly maps to.
            let name = if name == "NRST" {
                // Some packages share a GPIO with NRST.
                let shared_pin = package
                    .pins
                    .iter()
                    .find(|pin| pin.signals.iter().any(|s| s == "NRST") && pin.signals.len() > 1);

                match shared_pin {
                    Some(pin) => pin.signals.iter().find(|s| **s != "NRST").unwrap(),
                    None => name,
                }
            } else {
                name
            };

            pins.push(quote! {
                PeripheralPin {
                    pin: #name,
                    signal: #signal,
                    pf: #pf,
                }
            });
        }
    }

    let power_domain = match peripheral.power_domain {
        PowerDomain::Pd0 => quote! { PowerDomain::Pd0 },
        PowerDomain::Pd1 => quote! { PowerDomain::Pd1 },
        PowerDomain::Backup => quote! { PowerDomain::Backup },
    };

    let sys_fentries = match peripheral.sys_fentries {
        Some(sys_fentries) => quote! { Some(#sys_fentries) },
        None => quote! { None },
    };

    let interrupts = peripheral.interrupts.iter().map(|interrupt| {
        let name = &interrupt.name;
        let number = Literal::u32_unsuffixed(interrupt.num as u32);
        let group_iidx = match interrupt.group_iidx {
            Some(iidx) => {
                let iidx = Literal::u32_unsuffixed(iidx);
                quote! { Some(#iidx) }
            }
            None => quote! { None },
        };

        quote! {
            PeripheralInterrupt {
                name: #name,
                number: #number,
                group_iidx: #group_iidx,
            }
        }
    });

    let block_async = match peripheral.block_async {
        Some(block_async) => quote! { Some(#block_async) },
        None => quote! { None },
    };

    let retained_through = match peripheral.retained_through {
        Some(mode) => {
            let mode = power_mode(mode);
            quote! { Some(#mode) }
        }
        None => quote! { None },
    };

    let usable_through = match peripheral.usable_through {
        Some(mode) => {
            let mode = power_mode(mode);
            quote! { Some(#mode) }
        }
        None => quote! { None },
    };

    let clocked_in_standby1 = match peripheral.clocked_in_standby1 {
        Some(clocked) => quote! { Some(#clocked) },
        None => quote! { None },
    };

    let timer = match &peripheral.timer {
        Some(timer) => {
            let bits = Literal::u8_unsuffixed(timer.bits);
            let counters = Literal::u8_unsuffixed(timer.counters);
            let ccp_channels = Literal::u8_unsuffixed(timer.ccp_channels);
            let external_pwm_channels = Literal::u8_unsuffixed(timer.external_pwm_channels);
            let (prescaler, repeat_counter) = (timer.prescaler, timer.repeat_counter);
            let (phase_load, shadow_load, shadow_ccs) =
                (timer.phase_load, timer.shadow_load, timer.shadow_ccs);
            let (deadband, fault_handler, qei_hall) =
                (timer.deadband, timer.fault_handler, timer.qei_hall);

            quote! {
                Some(Timer {
                    bits: #bits,
                    counters: #counters,
                    prescaler: #prescaler,
                    repeat_counter: #repeat_counter,
                    ccp_channels: #ccp_channels,
                    external_pwm_channels: #external_pwm_channels,
                    phase_load: #phase_load,
                    shadow_load: #shadow_load,
                    shadow_ccs: #shadow_ccs,
                    deadband: #deadband,
                    fault_handler: #fault_handler,
                    qei_hall: #qei_hall,
                })
            }
        }
        None => quote! { None },
    };

    let clock_range_hz = match peripheral.clock_range_hz {
        Some(range) => {
            let min_hz = Literal::u32_unsuffixed(range.min_hz);
            let max_hz = Literal::u32_unsuffixed(range.max_hz);
            quote! { Some(ClockRange { min_hz: #min_hz, max_hz: #max_hz }) }
        }
        None => quote! { None },
    };

    let adc = match &peripheral.adc {
        Some(adc) => {
            let memctl = Literal::u8_unsuffixed(adc.memctl);
            let vrsel = Literal::u8_unsuffixed(adc.vrsel);
            let internal_channels = adc.internal_channels.iter().map(|(channel, source)| {
                let channel = Literal::u8_unsuffixed(*channel);
                let source = adc_internal_source(*source);
                quote! { AdcInternalChannel { channel: #channel, source: #source } }
            });
            quote! {
                Some(Adc {
                    memctl: #memctl,
                    vrsel: #vrsel,
                    internal_channels: &[#(#internal_channels),*],
                })
            }
        }
        None => quote! { None },
    };

    let unicomm = match peripheral.unicomm {
        Some(unicomm) => {
            let (uart, spi) = (unicomm.uart, unicomm.spi);
            let (i2c_controller, i2c_target) = (unicomm.i2c_controller, unicomm.i2c_target);
            quote! {
                Some(Unicomm {
                    uart: #uart,
                    i2c_controller: #i2c_controller,
                    i2c_target: #i2c_target,
                    spi: #spi,
                })
            }
        }
        None => quote! { None },
    };

    let vref = match peripheral.vref.and_then(|vref| vref.startup_ns) {
        Some(ns) => {
            let ns = Literal::u32_unsuffixed(ns);
            quote! { Some(Vref { startup_ns: Some(#ns) }) }
        }
        None => quote! { None },
    };

    Some(quote! {
        Peripheral {
            name: #name,
            kind: #kind,
            version: #version,
            pins: &[#(#pins),*],
            power_domain: #power_domain,
            sys_fentries: #sys_fentries,
            interrupts: &[#(#interrupts),*],
            block_async: #block_async,
            retained_through: #retained_through,
            usable_through: #usable_through,
            clocked_in_standby1: #clocked_in_standby1,
            timer: #timer,
            clock_range_hz: #clock_range_hz,
            adc: #adc,
            unicomm: #unicomm,
            vref: #vref,
        }
    })
}
