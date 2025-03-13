use heck::ToPascalCase;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use std::array;

use mspm0_data_types::{Chip, Interrupt};

pub fn generate(chip: &Chip) -> TokenStream {
    let interrupt_enum = interrupt_enum(chip);
    let group_enums = group_enums(chip);
    let rt_vectors = rt_vectors(chip);

    quote! {
        // FIXME define this from metadata
        #[cfg(feature = "rt")]
        pub const NVIC_PRIO_BITS: u32 = 2;
        #interrupt_enum
        #group_enums
        #rt_vectors
    }
}

fn interrupt_enum(chip: &Chip) -> TokenStream {
    let interrupts_variant = chip
        .interrupts
        .iter()
        // Ignore Cortex-M interrupts for enum
        .filter(|(_, interrupt)| interrupt.num >= 0)
        .map(|(_, interrupt)| {
        let name = Ident::new(&interrupt.name, Span::call_site());
        let value = Literal::u8_unsuffixed(interrupt.num as _);

        quote! { #name = #value }
    });

    quote! {
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub enum Interrupt {
            #(#interrupts_variant),*
        }

        unsafe impl cortex_m::interrupt::InterruptNumber for Interrupt {
            #[inline(always)]
            fn number(self) -> u16 {
                self as u16
            }
        }
    }
}

fn group_enums(chip: &Chip) -> TokenStream {
    chip.interrupts
        .values()
        .filter(|interrupt| !interrupt.group.is_empty())
        .map(group_enum)
        .collect()
}

fn group_enum(interrupt: &Interrupt) -> TokenStream {
    let name = Ident::new(&interrupt.name.to_pascal_case(), Span::call_site());

    let members = interrupt.group.iter().map(|(index, interrupt)| {
        let ident = Ident::new(&interrupt, Span::call_site());
        let value = Literal::u8_unsuffixed(*index as _);

        quote! {
            #ident = #value
        }
    });

    let matches = interrupt.group.iter().map(|(index, interrupt)| {
        let ident = Ident::new(&interrupt, Span::call_site());
        let value = Literal::u8_unsuffixed(*index as _);

        quote! {
            #value => Ok(Self::#ident),
        }
    });

    // let group_cpuss_offset = Literal::u32_unsuffixed(interrupt.group_cpuss_offset.unwrap());

    quote! {
        #[repr(u8)]
        pub enum #name {
            #(#members),*
        }

        impl TryFrom<u8> for #name {
            type Error = ();

            #[inline]
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    #(#matches)*
                    _ => Err(())
                }
            }
        }
    }
}

fn rt_vectors(chip: &Chip) -> TokenStream {
    let mut entries: [TokenStream; 32] = array::from_fn(|_| {
        quote! { Vector { _reserved: 0 } }
    });

    for interrupt in chip.interrupts.values() {
        let name = Ident::new(&interrupt.name, Span::call_site());

        // FIXME: Hack?
        if interrupt.num < 0 {
            continue;
        }

        // FIXME: Use exception number rather than NVIC number.
        entries[(interrupt.num) as usize] = quote! {
            Vector { _handler: #name }
        };
    }

    let fns = chip.interrupts.values().map(|interrupt| {
        let name = Ident::new(&interrupt.name, Span::call_site());
        quote! { fn #name (); }
    });

    // let _group_interrupt_handlers = chip
    //     .interrupts
    //     .iter()
    //     .filter(|interrupt| interrupt.group.is_some())
    //     .map(|group| rt_interrupt_group(&group.nvic.name, group.group.as_ref().unwrap()));

    quote! {
        #[cfg(feature = "rt")]
        mod _vectors {
            extern "C" {
                #(#fns)*
            }

            #[repr(C)]
            pub union Vector {
                _handler : unsafe extern "C" fn (),
                _reserved : u32,
            }

            #[link_section = ".vector_table.interrupts"]
            #[no_mangle]
            pub static __INTERRUPTS: [Vector; 32] = [
                #(#entries),*
            ];
        }

        #[cfg(feature = "rt")]
        pub use cortex_m_rt::interrupt;
        #[cfg(feature = "rt")]
        pub use Interrupt as interrupt;

        // TODO: Generate group handlers when more is done
        // #(#group_interrupt_handlers)*
    }
}
