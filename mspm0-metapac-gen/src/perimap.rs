use crate::util::RegexMap;

pub static PERIMAP: RegexMap<(&str, &str)> = RegexMap::new(&[
    (".*:uart", ("uart", "v1")),
    (".*:gpio", ("gpio", "v1")),
    (".*:dma", ("dma", "v1")),
    (".*:i2c", ("i2c", "v1")),
    (".*:beeper", ("beeper", "v1")),
    (".*:cpuss", ("cpuss", "v1")),
    (".*:iomux", ("iomux", "v1")),
    (".*:tim", ("tim", "v1")),
    ("mspm0c110x:sysctl", ("sysctl", "c110x")),
    ("mspm0g..0x:sysctl", ("sysctl", "g350x_g310x_g150x_g110x")),
    ("mspm0g..1x:sysctl", ("sysctl", "g351x_g151x")),
    ("mspm0l..0x:sysctl", ("sysctl", "l110x_l130x_l134x")),
    ("mspm0l134x:sysctl", ("sysctl", "l110x_l130x_l134x")),
    ("mspm0l..2x:sysctl", ("sysctl", "l122x_l222x")),
]);
