use std::{collections::HashSet, sync::LazyLock};

use mspm0_data_types::{Chip, Package, Peripheral, PeripheralType};
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

        Some(quote! { Pin { pin: #signal, pincm: #pincm } })
    });

    quote! { &[#(#pins),*] }
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
        if let Some(peri) = generate_peripheral(peri, &pins) {
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

pub fn adc_channels(chip: &Chip) -> TokenStream {
    let mut adc_channels = Vec::new();

    for (&adc_num, adc) in chip.adc_channels.iter() {
        let adc_number = Literal::u32_unsuffixed(adc_num);
        for (&num, _) in adc.iter() {
            let number = Literal::u32_unsuffixed(num);

            adc_channels.push(quote! {
                AdcChannel {
                    adc: #adc_number,
                    number: #number,
                }
            });
        }
    }

    quote! {
        &[#(#adc_channels),*]
    }
}

pub fn interrupts(chip: &Chip) -> TokenStream {
    let mut interrupts = Vec::new();

    for (_, interrupt) in chip.interrupts.iter() {
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

    for (_, interrupt) in chip.interrupts.iter() {
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

fn skip_peripheral(ty: PeripheralType) -> bool {
    matches!(ty, PeripheralType::Unknown | PeripheralType::Sysctl)
}

fn generate_peripheral(
    peripheral: &Peripheral,
    available_pins: &HashSet<String>,
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

        if available_pins.contains(name) {
            pins.push(quote! {
                PeripheralPin {
                    pin: #name,
                    signal: #signal,
                    pf: #pf,
                }
            });
        }
    }

    let mut attributes = Vec::<TokenStream>::new();

    if let Some(map) = &peripheral.attributes {
        for (name, value) in map {
            attributes.push(quote! {
                PeripheralAttribute {
                    name: #name,
                    value: #value,
                }
            })
        }
    }

    Some(quote! {
        Peripheral {
            name: #name,
            kind: #kind,
            version: #version,
            pins: &[#(#pins),*],
            attributes: &[#(#attributes),*],
        }
    })
}
