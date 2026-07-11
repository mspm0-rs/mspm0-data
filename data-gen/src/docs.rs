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
        "cc1312r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1312r",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz078",
        },
    ),
    (
        "cc1312psip",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1312psip",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz078",
        },
    ),
    (
        "cc1314r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1314r",
            reference: "https://www.ti.com/lit/pdf/swcu194",
            errata: "https://www.ti.com/lit/pdf/swrz133",
        },
    ),
    (
        "cc1350r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1350r",
            reference: "https://www.ti.com/lit/pdf/swcu117",
            errata: "https://www.ti.com/lit/pdf/swrz067",
        },
    ),
    (
        "cc1352r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1352r",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz077",
        },
    ),
    (
        "cc1352p7(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1352p7",
            reference: "https://www.ti.com/lit/pdf/swcu192",
            errata: "https://www.ti.com/lit/pdf/swrz109",
        },
    ),
    (
        "cc1352p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1352p",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz076",
        },
    ),
    (
        "cc1354p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1354p10",
            reference: "https://www.ti.com/lit/pdf/swcu194",
            errata: "https://www.ti.com/lit/pdf/swrz132",
        },
    ),
    (
        "cc1354r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc1354r10",
            reference: "https://www.ti.com/lit/pdf/swcu194",
            errata: "https://www.ti.com/lit/pdf/swrz131",
        },
    ),
    (
        "cc2340r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2340r5",
            reference: "https://www.ti.com/lit/pdf/swcu193",
            errata: "https://www.ti.com/lit/pdf/swrz134",
        },
    ),
    (
        "cc2640r2f(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2640r2f",
            reference: "https://www.ti.com/lit/pdf/swcu117",
            errata: "https://www.ti.com/lit/pdf/swrz085",
        },
    ),
    (
        "cc2642r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2642r",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz079",
        },
    ),
    (
        "cc2651r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2651r3",
            reference: "https://www.ti.com/lit/pdf/swcu191",
            errata: "https://www.ti.com/lit/pdf/swrz121",
        },
    ),
    (
        "cc2651p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2651p3",
            reference: "https://www.ti.com/lit/pdf/swcu191",
            errata: "https://www.ti.com/lit/pdf/swrz120",
        },
    ),
    (
        "cc2652p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2652p",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz081",
        },
    ),
    (
        "cc2652r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2652r",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz080",
        },
    ),
    (
        "cc2662r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2662r-q1",
            reference: "https://www.ti.com/lit/pdf/swcu185",
            errata: "https://www.ti.com/lit/pdf/swrz107",
        },
    ),
    (
        "cc2674r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2674r10",
            reference: "https://www.ti.com/lit/pdf/swcu194",
            errata: "https://www.ti.com/lit/pdf/swrz129",
        },
    ),
    (
        "cc2674p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2674p10",
            reference: "https://www.ti.com/lit/pdf/swcu194",
            errata: "https://www.ti.com/lit/pdf/swrz130",
        },
    ),
    (
        "cc274(4|5)r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2744r7-q1",
            reference: "https://www.ti.com/lit/pdf/swcu195",
            errata: "https://www.ti.com/lit/pdf/swrz161",
        },
    ),
    (
        "cc2745p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2745p10-q1",
            reference: "https://www.ti.com/lit/pdf/swcu195",
            errata: "https://www.ti.com/lit/pdf/swrz161",
        },
    ),
    (
        "cc2755p(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2755p10",
            reference: "https://www.ti.com/lit/pdf/swcu195",
            errata: "https://www.ti.com/lit/pdf/swrz171",
        },
    ),
    (
        "cc2755r(.*)",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc2755r10",
            reference: "https://www.ti.com/lit/pdf/swcu195",
            errata: "https://www.ti.com/lit/pdf/swrz171",
        },
    ),
    (
        "cc3551e",
        Links {
            datasheet: "https://www.ti.com/lit/gpn/cc3551e",
            reference: "https://www.ti.com/lit/pdf/swru626",
            errata: "https://www.ti.com/lit/pdf/swrz167",
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
