#![no_std]
#![allow(non_snake_case)]
#![allow(unused)]
#![allow(non_camel_case_types)]
// chiptool generates enum-like fieldset values in Pascal case, including the ones it emits as
// associated constants rather than enum variants.
#![allow(non_upper_case_globals)]
#![doc(html_no_source)]

pub mod common;

#[cfg(feature = "pac")]
include!(env!("MSPM0_METAPAC_PAC_PATH"));

#[cfg(feature = "metadata")]
pub mod metadata {
    include!("metadata.rs");
    include!(env!("MSPM0_METAPAC_METADATA_PATH"));
    include!("all_chips.rs");
    include!("all_peripheral_versions.rs");
}
