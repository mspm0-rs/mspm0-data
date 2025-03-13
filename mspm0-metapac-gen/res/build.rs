use std::env;
#[cfg(any(feature = "rt", feature = "memory-x"))]
use std::path::PathBuf;

enum GetOneError {
    None,
    Multiple,
}

trait IteratorExt: Iterator {
    fn get_one(self) -> Result<Self::Item, GetOneError>;
}

impl<T: Iterator> IteratorExt for T {
    fn get_one(mut self) -> Result<Self::Item, GetOneError> {
        match self.next() {
            None => Err(GetOneError::None),
            Some(res) => match self.next() {
                Some(_) => Err(GetOneError::Multiple),
                None => Ok(res),
            },
        }
    }
}

fn main() {
    #[cfg(any(feature = "rt", feature = "memory-x"))]
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let chip_name = match env::vars()
        .map(|(a, _)| a)
        .filter(|x| x.starts_with("CARGO_FEATURE_MSPM0") || x.starts_with("CARGO_FEATURE_MSPS"))
        .get_one()
    {
        Ok(x) => x,
        Err(GetOneError::None) => panic!("No mspm0xx/mspsxx Cargo feature enabled"),
        Err(GetOneError::Multiple) => panic!("Multiple mspm0xx/mspsxx Cargo features enabled"),
    }
    .strip_prefix("CARGO_FEATURE_")
    .unwrap()
    .to_ascii_lowercase()
    .replace('_', "-");

    #[cfg(feature = "rt")]
    println!(
        "cargo:rustc-link-search={}/src/chips/{}",
        crate_dir.display(),
        chip_name,
    );

    #[cfg(feature = "memory-x")]
    println!(
        "cargo:rustc-link-search={}/src/chips/{}/memory_x/",
        crate_dir.display(),
        chip_name,
    );

    println!("cargo:rustc-env=MSPM0_METAPAC_PAC_PATH=chips/{}/pac.rs", chip_name);
    println!(
        "cargo:rustc-env=MSPM0_METAPAC_METADATA_PATH=chips/{}/metadata.rs",
        chip_name,
    );

    println!("cargo:rerun-if-changed=build.rs");
}
