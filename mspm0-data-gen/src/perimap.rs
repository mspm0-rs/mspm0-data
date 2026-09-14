use crate::util::RegexMap;

pub static PERIMAP: RegexMap<&str> = RegexMap::new(&[
    (".*:aesadv", "v1"),
    (".*:uart", "v1"),
    // One version: hw_spi.h is a single shared IP description and every legacy instance has the
    // whole of it, including the DMA trigger groups the C110x SVD leaves out.
    (".*:spi", "v1"),
    (".*:gpio", "v1"),
    (".*:dma", "v1"),
    (".*:i2c", "v1"),
    (".*:beeper", "v1"),
    (".*:cpuss", "v1"),
    (".*:iomux", "v1"),
    // Two subscriber ports and nothing else; every family has the same pair.
    (".*:wuc", "v1"),
    // One version: hw_debugss.h is a single shared IP description and SLAU893 Table 22-11 lists
    // the same fourteen registers. The C110x SVD's extra PINCM/CPU_CONNECT_0 are not in either.
    (".*:debugss", "v1"),
    (".*:mathacl", "v1"),
    (".*:opa", "v1"),
    // A TIMB instance is a basic timer: its counters live at 0x1100 where the general-purpose
    // block has CCPD/ODIS/CCLKCTL, and it has neither CLKDIV/CLKSEL nor a COUNTERREGS group.
    // Keyed on the instance kind, not the peripheral type — see `get_peripheral_type_version`.
    (".*:timb", "btimer"),
    (".*:tim", "v1"),
    (".*:adc", "v1"),
    // Only g150x, g151x, g350x and g351x have a DAC0, so an ungated entry cannot over-match.
    // Not the same four families as the OPA: the Gx51x parts have a DAC and no amplifier.
    (".*:dac", "v1"),
    (".*:wwdt", "v1"),
    (".*:flashctl", "v1"),
    (".*:trng", "v1"),
    (".*:canfd", "v1"),
    // One layout, but DACOUTEN (CTL1 bit 11) only exists where the header states
    // `COMP_SYS_DACOUT_EN`: the 8-bit reference DAC reaches a pin on these four families only.
    ("mspm0c1105_c1106:comp", "dacout"),
    ("mspm0g518x:comp", "dacout"),
    ("mspm0l112x:comp", "dacout"),
    ("mspm0l211x:comp", "dacout"),
    (".*:comp", "v1"),
    // Three CRC generators. `16` lacks CRCCTRL.POLYSIZE: those devices are CRC-16 only, stated
    // per device by the headers' `CRC_SYS_CRC32_ENABLE = 0`. `p` adds the programmable-polynomial
    // CRCPOLY register; TI types those instances `CRCP_Regs` and the TRMs call them CRCP0.
    ("mspm0c110x:crc", "16"),
    ("msps003fx:crc", "16"),
    ("mspm0c1105_c1106:crc", "16"),
    ("mspm0h321x:crc", "16"),
    ("mspm0l112x:crc", "16"),
    ("mspm0l211x:crc", "16"),
    ("mspm0g..1x:crc", "p"),
    ("mspm0g518x:crc", "p"),
    ("mspm0l.22x:crc", "p"),
    (".*:crc", "v1"),
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
    // No RTC entry: a version names a register block, and none has been curated. What the split
    // actually is, settled against hw_rtc.h, hw_lfss.h, SLAU846 SS37.4 and the SVDs:
    //
    // The RTC calendar registers are byte-identical everywhere - CLKCTL through RTCLOCK, 1100h to
    // 11FCh, same names and same offsets in both headers, at the same absolute address since every
    // instance is based at 0x40094000. The two blocks differ in what surrounds them, not in the
    // RTC. The legacy peripheral on `mspm0g..0x` has a GPRCM (PWREN/RSTCTL/CLKCFG/STAT), a
    // publisher port and no subscriber, and its GEN_EVENT group is spaced 4 bytes where every
    // other MSPM0 interrupt group is spaced 8 - hw_rtc.h and the TRM's register table both say so,
    // and the SVD is the one that disagrees. The LFSS has no GPRCM at all, has FSUB_0 as well as
    // FPUB_0, spaces both interrupt groups 8 bytes, and wraps the same RTC group together with
    // tamper I/O (1200h), a watchdog (1300h) and scratch-pad memory (1400h).
    //
    // So `rtc_b` should not exist as a block: on g151x, g351x, g518x, l112x and l211x sysconfig
    // emits `RTC_B` and `LFSS` as two instances at the same address, and they are one
    // peripheral. The work is `rtc_legacy` for the four `mspm0g..0x` families and `lfss_v1` for
    // the nine that have an LFSS, after which the duplicate `RTC_B` instance should go.
    //
    // Nor is sysconfig's `RTC_B` node a reliable "has an RTC" signal: c1105_c1106, h321x, l122x
    // and l222x have an LFSS with no `RTC_B` node, and all four datasheets advertise an RTC.
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
    // L112x/L211x likewise borrow the L122x SYSCTL until their own reference manual lands.
    // They are known to differ already: both have a beeper, which l122x and l222x do not.
    ("mspm0l112x:sysctl", "l122x_l222x"),
    ("mspm0l211x:sysctl", "l122x_l222x"),
]);
