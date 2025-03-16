use mspm0_data_types::{Chip, Peripheral, PeripheralType};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub fn generate(name: &str, chip: &Chip) -> TokenStream {
    let mut peripherals = Vec::<TokenStream>::new();

    for peri in chip.peripherals.values() {
        if let Some(peri) = generate_peripheral(peri) {
            peripherals.push(peri);
        }
    }

    let mut pincm_mappings = Vec::new();

    for (pin, cm) in chip.iomux.iter() {
        let pincm = Literal::u8_unsuffixed(*cm as _);

        pincm_mappings.push(quote! {
            PinCmMapping {
                pin: #pin,
                pincm: #pincm,
            }
        });
    }

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
        pub static METADATA: Metadata = Metadata {
            name: #name,
            peripherals: &[#(#peripherals),*],
            pincm_mappings: &[#(#pincm_mappings),*],
            interrupts: &[#(#interrupts),*],
            dma_channels: &[#(#dma_channels),*],
        };
    }
}

fn skip_peripheral(ty: PeripheralType) -> bool {
    matches!(ty, PeripheralType::Unknown | PeripheralType::Sysctl)
}

fn generate_peripheral(peripheral: &Peripheral) -> Option<TokenStream> {
    // Exclude peripherals that don't really exist as singletons.
    if skip_peripheral(peripheral.ty) {
        return None;
    }

    let name = &peripheral.name;
    let kind = &peripheral.ty.to_string();

    let mut pins = Vec::<TokenStream>::new();

    for pin in peripheral.pins.iter() {
        let name = &pin.pin;
        let signal = &pin.signal;
        let pf = match pin.pf {
            Some(pf) => quote! { Some(#pf) },
            None => quote! { None },
        };

        pins.push(quote! {
            PeripheralPin {
                pin: #name,
                signal: #signal,
                pf: #pf,
            }
        });
    }

    Some(quote! {
        Peripheral {
            name: #name,
            kind: #kind,
            pins: &[#(#pins),*],
        }
    })
}
