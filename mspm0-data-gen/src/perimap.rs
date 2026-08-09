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
    ("mspm0c110x:sysctl", "c110x"),
    ("mspm0c1105_c1106:sysctl", "c110x"),
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
