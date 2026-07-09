//! RegexSets to find documentation for a chip.

use crate::util::RegexMap;

pub struct Links {
    pub datasheet: &'static str,
    pub reference: &'static str,
    pub errata: &'static str,
}

pub static DOCS: RegexMap<Links> = RegexMap::new(&[
    (
        "cc1310(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1310",
            reference: "https://www.ti.com/lit/pdf/swcu117",
            errata: "https://www.ti.com/lit/pdf/swrz062",
        },
    ),
    (
        "cc1311p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1311p3",
            reference: "https://www.ti.com/lit/pdf/swcu191",
            errata: "https://www.ti.com/lit/pdf/swrz118",
        },
    ),
    (
        "cc1311r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1311r3",
            reference: "https://www.ti.com/lit/pdf/swcu191",
            errata: "https://www.ti.com/lit/pdf/swrz118",
        },
    ),
    (
        "msps003f(.*)|mspm0c110(3|4)(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/mspm0c1104",
            reference: "https://www.ti.com/lit/pdf/slau893",
            errata: "https://www.ti.com/lit/pdf/slaz753",
        },
    ),
    (
        "mspm0c110(5|6)(.*)|msp32(g|c)031(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/mspm0c1106",
            reference: "https://www.ti.com/lit/pdf/slau893",
            errata: "https://www.ti.com/lit/pdf/slaz761",
        },
    ),
]);
