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
    (".*:factoryregion", "v1"),
    // SLAU893 describes two C-series SYSCTLs, "SYSCTL_C1103_C1104" and "SYSCTL_C1105_C1106". The
    // latter is a superset: it adds the HFXT and LFXT crystal drivers, `MCLKCFG.FLASHWAIT` and the
    // HSCLK mux, which its datasheet also specifies and MSPM0C1104's does not.
    ("mspm0c110x:sysctl", "c110x"),
    ("mspm0c1105_c1106:sysctl", "c1105_c1106"),
    ("msps003fx:sysctl", "c110x"),
    ("mspm0g..0x:sysctl", "g350x_g310x_g150x_g110x"),
    ("mspm0g..1x:sysctl", "g351x_g151x"),
    // Derived from the G351x block: same flash-protection and security region, minus the SRAM
    // bank-1 registers and CANCLKSRC, plus the USB FLL. TI's own header and SVD for the family
    // describe it; only the TRM still lags.
    ("mspm0g518x:sysctl", "g518x"),
    ("mspm0h321x:sysctl", "h321x"),
    ("mspm0l..0x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l134x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l.22x:sysctl", "l122x_l222x"),
    // FIXME: When reference manual is updated for L112/L211x, update these if needed (split out).
    ("mspm0l112x:sysctl", "l122x_l222x"),
    ("mspm0l211x:sysctl", "l122x_l222x"),
]);
