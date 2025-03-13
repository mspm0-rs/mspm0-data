use heck::ToPascalCase;
use mspm0_data_types::{Chip, Peripheral, PeripheralType};
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

/// Which peripherals should be generated.
///
/// By name, then version (if any)
const GENERATE_PERIPHERALS: &[PeripheralType] = &[
    PeripheralType::Cpuss,
    PeripheralType::Gpio,
    PeripheralType::Iomux,
    PeripheralType::Sysctl,
    PeripheralType::Tim,
];

pub fn generate(chip: &Chip) -> TokenStream {
    let peripheral_imports = generate_peripheral_imports(chip);
    let peripheral_consts = generate_peripheral_consts(chip);

    quote! {
        #peripheral_imports
        #peripheral_consts
    }
}

fn generate_peripheral_imports(chip: &Chip) -> TokenStream {
    // Sort the peripherals by type for generation.
    let mut peripheral_types = chip.peripherals.iter().collect::<Vec<_>>();
    peripheral_types.sort_by(|(a, _), (b, _)| a.cmp(&b));
    peripheral_types.dedup_by(|(_, a), (_, b)| a.ty == b.ty);

    peripheral_types
        .iter()
        .map(|(name, peripheral)| generate_import(name, peripheral))
        .collect()
}

fn generate_import(_name: &str, peripheral: &Peripheral) -> TokenStream {
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

    let path = if let Some(ref version) = peripheral.version {
        format!("../../peripherals/{}_{}.rs", peripheral.ty, version)
    } else {
        format!("../../peripherals/{}.rs", peripheral.ty)
    };

    quote! {
        #[path = #path]
        pub mod #name;
    }
}

fn generate_peripheral_consts(chip: &Chip) -> TokenStream {
    chip.peripherals.values().map(generate_const).collect()
}

fn generate_const(peripheral: &Peripheral) -> TokenStream {
    let name = Ident::new(&peripheral.name, Span::call_site());

    if !GENERATE_PERIPHERALS.iter().any(|ty| ty == &peripheral.ty) {
        let comment = format!("Address: {}", peripheral.address);
        return quote! {
            #[doc = #comment]
            pub const #name: () = ();
        };
    }

    let address = Literal::u32_unsuffixed(peripheral.address);
    let module = Ident::new(&format!("{}", peripheral.ty), Span::call_site());
    let ty = Ident::new(
        &format!("{}", peripheral.ty).to_pascal_case(),
        Span::call_site(),
    );

    quote! {
        pub const #name: #module::#ty = unsafe { #module::#ty::from_ptr(#address as *mut _) };
    }
}
