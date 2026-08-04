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
    (".*:tim", "v1"),
    (".*:adc", "v1"),
    (".*:wwdt", "v1"),
    (".*:flashctl", "v1"),
    (".*:trng", "v1"),
    (".*:canfd", "v1"),
    (".*:factoryregion", "v1"),
    // SLAU893 describes two C-series SYSCTLs, "SYSCTL_C1103_C1104" and "SYSCTL_C1105_C1106". The
    // latter is a superset: it adds the HFXT and LFXT crystal drivers, `MCLKCFG.FLASHWAIT` and the
    // HSCLK mux, which its datasheet also specifies and MSPM0C1104's does not.
    ("mspm0c110x:sysctl", "c110x"),
    ("mspm0c1105_c1106:sysctl", "c1105_c1106"),
    ("msps003fx:sysctl", "c110x"),
    ("mspm0g..0x:sysctl", "g350x_g310x_g150x_g110x"),
    ("mspm0g..1x:sysctl", "g351x_g151x"),
    // FIXME: When reference manual is updated for G511x/G518x, update this if needed.
    ("mspm0g5..x:sysctl", "g350x_g310x_g150x_g110x"),
    ("mspm0h321x:sysctl", "h321x"),
    ("mspm0l..0x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l134x:sysctl", "l110x_l130x_l134x"),
    ("mspm0l.22x:sysctl", "l122x_l222x"),
    // FIXME: When reference manual is updated for L112/L211x, update these if needed (split out).
    ("mspm0l112x:sysctl", "l122x_l222x"),
    ("mspm0l211x:sysctl", "l122x_l222x"),
]);
