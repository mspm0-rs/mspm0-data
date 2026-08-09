use crate::util::RegexMap;

pub static PERIMAP: RegexMap<&str> = RegexMap::new(&[
    (".*:uart", "v1"),
    (".*:gpio", "v1"),
    (".*:dma", "v1"),
    (".*:i2c", "v1"),
    (".*:beeper", "v1"),
    (".*:cpuss", "v1"),
    (".*:iomux", "v1"),
    (".*:mathacl", "v1"),
    (".*:opa", "v1"),
    // A TIMB instance is a basic timer: its counters live at 0x1100 where the general-purpose
    // block has CCPD/ODIS/CCLKCTL, and it has neither CLKDIV/CLKSEL nor a COUNTERREGS group.
    // Keyed on the instance kind, not the peripheral type — see `get_peripheral_type_version`.
    (".*:timb", "btimer"),
    (".*:tim", "v1"),
    (".*:adc", "v1"),
    (".*:wwdt", "v1"),
    (".*:flashctl", "v1"),
    (".*:trng", "v1"),
    (".*:canfd", "v1"),
    // One version: the SDK ships a single hw_unicomm.h for the portfolio, and the wrapper is what
    // every instance has regardless of which mode maps it implements.
    (".*:unicomm", "v1"),
    // The mode maps of a UNICOMM instance. The SVDs describe them per instance and identically, so
    // one version each covers the portfolio, as with the wrapper.
    (".*:unicommi2cc", "v1"),
    (".*:unicommi2ct", "v1"),
    (".*:unicommspi", "v1"),
    (".*:unicommuart", "v1"),
    // SLAU846 §23.3, SLAU847 §19.3, SLAU893 §13.3 and SLAU923 §11.3 describe the same eight
    // registers with the same fields, so every family shares one version. The SVDs disagree with
    // the TRMs and with each other about CTL0 bits 1 and 2, which the YAML resolves in the TRMs'
    // favour.
    (".*:vref", "v1"),
    // No RTC entry: a version names a register block, and none has been curated. The variants,
    // for whoever writes them: sysconfig calls the standalone pre-LFSS peripheral the "legacy"
    // RTC, and the ones inside the LFSS are RTC_A when the LFSS has an independent VBAT supply
    // and RTC_B when it is powered from VDD. `mspm0g..0x` has the legacy one, everything else B.
    (".*:factoryregion", "v1"),
    // SLAU893 describes two C-series SYSCTLs, "SYSCTL_C1103_C1104" and "SYSCTL_C1105_C1106". The
    // latter is a superset: it adds the HFXT and LFXT crystal drivers, `MCLKCFG.FLASHWAIT` and the
    // HSCLK mux, which its datasheet also specifies and MSPM0C1104's does not.
    ("mspm0c110x:sysctl", "c110x"),
    ("mspm0c1105_c1106:sysctl", "c1105_c1106"),
    ("msps003fx:sysctl", "c110x"),
    ("mspm0g..0x:sysctl", "g350x_g310x_g150x_g110x"),
    ("mspm0g..1x:sysctl", "g351x_g151x"),
    // G511x/G518x have no reference manual of their own yet, so they borrow the G350x SYSCTL.
    // Split them out when one is published.
    ("mspm0g5..x:sysctl", "g350x_g310x_g150x_g110x"),
    ("mspm0h321x:sysctl", "h321x"),
    ("mspm0l..0x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l134x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l.22x:sysctl", "l122x_l222x"),
    // L112x/L211x likewise borrow the L122x SYSCTL until their own reference manual lands.
    // They are known to differ already: both have a beeper, which l122x and l222x do not.
    ("mspm0l112x:sysctl", "l122x_l222x"),
    ("mspm0l211x:sysctl", "l122x_l222x"),
]);
