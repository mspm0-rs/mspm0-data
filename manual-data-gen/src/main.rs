use std::{ffi::OsStr, fs, path::PathBuf, sync::LazyLock};

use anyhow::{Context, bail};
use manual_data_gen::{DocType, ManualFile};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from("build/data2");
    let data_dir = PathBuf::from("data");

    let data_manual_dir = data_dir.join("manual");
    let read_dir = fs::read_dir(&data_manual_dir).context("read manual data directory")?;

    let mut entries = Vec::new();

    for entry in read_dir {
        let entry = entry.context("get entry in manual data directory")?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("check file type of {}", path.display()))?;
        let is_yaml = path.extension() == Some(OsStr::new("yaml"));

        if file_type.is_file() && is_yaml {
            entries.push(path);
        }
    }

    let files = entries
        .par_iter()
        .map(|path| {
            let file = match fs::File::open(path) {
                Ok(f) => f,
                Err(e) => {
                    return (
                        path,
                        Err(e).with_context(|| format!("opening {}", path.display())),
                    );
                }
            };

            let file = match serde_yaml::from_reader::<_, ManualFile>(file) {
                Ok(f) => f,
                Err(e) => {
                    return (
                        path,
                        Err(e).with_context(|| format!("deserialize {}", path.display())),
                    );
                }
            };

            (path, Ok(file))
        })
        .collect::<Vec<_>>();

    if files.iter().any(|(_, result)| result.is_err()) {
        println!("Error reading manual data files");

        for (path, result) in files.into_iter() {
            if let Err(err) = result {
                println!("For {}: {}", path.display(), err);
            }
        }

        bail!("Error reading sysconfig package data (see above)");
    }

    // Remove error type
    let files = files
        .into_iter()
        .map(|(path, result)| (path, result.unwrap()))
        .collect::<Vec<_>>();

    warn_missing_important_docs_and_validate_urls(&files);

    Ok(())
}

fn warn_missing_important_docs_and_validate_urls(files: &Vec<(&PathBuf, ManualFile)>) {
    static REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"https://www\.ti\.com/lit/(gpn|pdf)/\w+").unwrap());

    for (path, file) in files.iter() {
        let name = path.file_stem().unwrap().to_string_lossy();
        let datasheet = file.docs.get(&DocType::Datasheet);
        let reference_manual = file.docs.get(&DocType::ReferenceManual);
        let errata = file.docs.get(&DocType::Errata);

        match datasheet {
            Some(url) => {
                if !REGEX.is_match(url) {
                    println!("warn: {} datasheet has malformed url", name);
                }
            }
            None => println!("warn: {} has no datasheet field", name),
        }

        match reference_manual {
            Some(url) => {
                if !REGEX.is_match(url) {
                    println!("warn: {} reference_manual has malformed url", name);
                }
            }
            None => println!("warn: {} has no reference_manual field", name),
        }

        match errata {
            Some(url) => {
                if !REGEX.is_match(url) {
                    println!("warn: {} errata has malformed url", name);
                }
            }
            None => println!("warn: {} has no errata field", name),
        }

        if let Some(url) = file.docs.get(&DocType::SafetyManual)
            && !REGEX.is_match(url)
        {
            println!("warn: {} safety_manual has malformed url", name);
        }
    }
}
