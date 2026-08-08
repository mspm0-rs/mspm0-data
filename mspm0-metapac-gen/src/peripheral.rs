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
    PeripheralType::FactoryRegion,
    PeripheralType::FlashCtl,
    PeripheralType::Gpio,
    PeripheralType::I2c,
    PeripheralType::Iomux,
    PeripheralType::Mathacl,
    PeripheralType::Opa,
    PeripheralType::Sysctl,
    PeripheralType::Tim,
    PeripheralType::Trng,
    PeripheralType::Uart,
    PeripheralType::Unicomm,
    PeripheralType::UnicommI2cc,
    PeripheralType::UnicommSpi,
    PeripheralType::UnicommUart,
    PeripheralType::Vref,
    PeripheralType::Wwdt,
];

/// Register block versions which do not get the module named after their peripheral type.
///
/// One entry per version that has to coexist with another version of the same type on one chip.
/// TIMB is a basic timer: it has its own register block but sits alongside the general-purpose
/// TIMA and TIMG instances, which keep `tim`.
///
/// Peripheral type, version, module name.
const VARIANT_MODULES: &[(&str, &str, &str)] = &[("tim", "btimer", "timb")];

pub fn generate(chip: &Chip, all_versions: &mut BTreeMap<String, BTreeSet<String>>) -> TokenStream {
    let peripheral_imports = generate_peripheral_imports(chip, all_versions);
    let peripheral_consts = generate_peripheral_consts(chip);

    quote! {
        #peripheral_imports
        #peripheral_consts
    }
}

/// The register block versions this chip needs, per peripheral type.
///
/// A type can need more than one: TIMB is a basic timer and does not share the general-purpose
/// block that TIMA and TIMG use, so a chip with both needs two timer blocks generated.
fn chip_versions(chip: &Chip) -> BTreeMap<String, BTreeSet<String>> {
    let mut versions = BTreeMap::<String, BTreeSet<String>>::new();

    for peripheral in chip.peripherals.values() {
        if !GENERATE_PERIPHERALS.contains(&peripheral.ty) {
            continue;
        }

        if let Some(version) = &peripheral.version {
            versions
                .entry(peripheral.ty.to_string())
                .or_default()
                .insert(version.clone());
        }
    }

    versions
}

/// Name of the module a register block is imported as.
///
/// The module is normally named after the peripheral type, since a chip has one register block per
/// type. `VARIANT_MODULES` names the versions that are the exception, so that the type keeps its
/// own module name on every chip and a chip which also has the variant just gains a second module.
fn module_name<'a>(ty: &'a str, version: &str) -> &'a str {
    VARIANT_MODULES
        .iter()
        .find(|(variant_ty, variant_version, _)| *variant_ty == ty && *variant_version == version)
        .map(|(_, _, module)| *module)
        .unwrap_or(ty)
}

fn generate_peripheral_imports(
    chip: &Chip,
    all_versions: &mut BTreeMap<String, BTreeSet<String>>,
) -> TokenStream {
    let mut modules = BTreeMap::<String, (String, String)>::new();

    for (ty, versions) in chip_versions(chip) {
        all_versions
            .entry(ty.clone())
            .or_default()
            .extend(versions.iter().cloned());

        for version in versions {
            let module = module_name(&ty, &version).to_owned();

            if let Some((_, other)) = modules.insert(module.clone(), (ty.clone(), version.clone()))
            {
                // Two register blocks cannot share a module. Whichever version is the odd one out
                // needs an entry in `VARIANT_MODULES`.
                panic!(
                    "{}: {ty} versions {other} and {version} both want the module {module}",
                    chip.name
                );
            }
        }
    }

    modules
        .iter()
        .map(|(module, (ty, version))| {
            let module = Ident::new(module, Span::call_site());
            let path = format!("../../peripherals/{ty}_{version}.rs");

            quote! {
                #[path = #path]
                pub mod #module;
            }
        })
        .collect()
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
    let ty_name = peripheral.ty.to_string();
    let module = peripheral
        .version
        .as_deref()
        .map_or(ty_name.as_str(), |version| module_name(&ty_name, version));
    let module = Ident::new(module, Span::call_site());
    let ty = Ident::new(&ty_name.to_pascal_case(), Span::call_site());

    quote! {
        pub const #name: #module::#ty = unsafe { #module::#ty::from_ptr(#address as *mut _) };
    }
}
