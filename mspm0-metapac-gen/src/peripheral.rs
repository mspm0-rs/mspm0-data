use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase;
use mspm0_data_types::{Chip, Peripheral, PeripheralType};
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

/// Which peripherals should be generated.
///
/// By name, then version (if any)
const GENERATE_PERIPHERALS: &[PeripheralType] = &[
    PeripheralType::Adc,
    PeripheralType::Cpuss,
    PeripheralType::Dma,
    PeripheralType::Canfd,
    PeripheralType::Gpio,
    PeripheralType::I2c,
    PeripheralType::Iomux,
    PeripheralType::Mathacl,
    PeripheralType::Sysctl,
    PeripheralType::Tim,
    PeripheralType::Trng,
    PeripheralType::Uart,
    PeripheralType::Wwdt,
];

pub fn generate(chip: &Chip, all_versions: &mut BTreeMap<String, BTreeSet<String>>) -> TokenStream {
    let peripheral_imports = generate_peripheral_imports(chip, all_versions);
    let peripheral_consts = generate_peripheral_consts(chip);

    quote! {
        #peripheral_imports
        #peripheral_consts
    }
}

fn generate_peripheral_imports(
    chip: &Chip,
    all_versions: &mut BTreeMap<String, BTreeSet<String>>,
) -> TokenStream {
    // Sort the peripherals by type for generation.
    let mut peripheral_types = chip.peripherals.iter().collect::<Vec<_>>();
    peripheral_types.sort_by(|(a, _), (b, _)| a.cmp(b));
    peripheral_types.dedup_by(|(_, a), (_, b)| a.ty == b.ty);

    peripheral_types
        .iter()
        .map(|(name, peripheral)| generate_import(chip, name, peripheral, all_versions))
        .collect()
}

fn generate_import(
    _chip: &Chip,
    _name: &str,
    peripheral: &Peripheral,
    all_versions: &mut BTreeMap<String, BTreeSet<String>>,
) -> TokenStream {
    if !GENERATE_PERIPHERALS
        .iter()
        .any(|ty_generate| &peripheral.ty == ty_generate)
    {
        return TokenStream::default();
    }

    let name = Ident::new(
        &format!("{}", peripheral.ty).to_lowercase(),
        Span::call_site(),
    );

    if let Some(version) = peripheral.version.clone() {
        all_versions
            .entry(name.to_string())
            .or_default()
            .insert(version.clone());
        let path = format!("../../peripherals/{}_{}.rs", name, version);
        quote! {
            #[path = #path]
            pub mod #name;
        }
    } else {
        TokenStream::default()
    }
}

fn generate_peripheral_consts(chip: &Chip) -> TokenStream {
    chip.peripherals.values().map(generate_const).collect()
}

fn generate_const(peripheral: &Peripheral) -> TokenStream {
    let name = Ident::new(&peripheral.name, Span::call_site());

    // Some peripherals live in other peripherals. Like GPAMP living in SYSCTL.
    //
    // For now the HAL determines the actual address of these special peripherals.
    let Some(address) = peripheral.address else {
        return TokenStream::new();
    };

    if !GENERATE_PERIPHERALS.iter().any(|ty| ty == &peripheral.ty) {
        let comment = format!("Address: {}", address);
        return quote! {
            #[doc = #comment]
            pub const #name: () = ();
        };
    }

    let address = Literal::u32_unsuffixed(address);
    let module = Ident::new(&format!("{}", peripheral.ty), Span::call_site());
    let ty = Ident::new(
        &format!("{}", peripheral.ty).to_pascal_case(),
        Span::call_site(),
    );

    quote! {
        pub const #name: #module::#ty = unsafe { #module::#ty::from_ptr(#address as *mut _) };
    }
}
