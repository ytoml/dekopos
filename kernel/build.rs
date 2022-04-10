use std::collections::HashSet;
use std::env;
use std::io::{BufRead, Write};
use std::num::ParseIntError;
use std::path::Path;
use std::process::Command;
use std::{fs::OpenOptions, io::BufReader};

use thiserror::Error;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type LineResult<T> = std::result::Result<T, LineError>;

const SRC: &str = "assets/hankaku.txt";
const OUT: &str = "ascii.rs";

#[derive(Error, Debug)]
enum LineError {
    #[error("Too much elements: {0:?}")]
    ExcessiveElements(String),
    #[error("Invalid lenghth of ascii line: {0:?}")]
    InvalidLengthBody(String),
    #[error("Unexpected char found: {0:?}")]
    UnexpectedChar(char),
    #[error("Unexpected char found: {0:?}")]
    Parse(#[from] ParseIntError),
}

#[derive(Debug)]
enum Line {
    Definition(u8),
    Body(u8),
}

impl Line {
    const ASCII_WIDTH: usize = 8;
    const ASCII_HEIGHT: usize = 16;

    pub fn from_str<S>(string: S) -> Result<Option<Self>>
    where
        S: AsRef<str>,
    {
        let string = string.as_ref().trim();
        if string.is_empty() {
            return Ok(None);
        }
        let tokens = string.split(' ').collect::<Vec<_>>();
        let line = match tokens.len() {
            0 => None,
            _i @ 1..=2 => Some(Self::parse(tokens[0])?),
            _ => return Err(Box::new(LineError::ExcessiveElements(string.into()))),
        };
        Ok(line)
    }

    fn parse(s: &str) -> LineResult<Self> {
        // argument s is required not to be empty.
        let s = s.trim(); // Just in case
        match s.chars().next().unwrap() {
            '0' => {
                let s = s
                    .strip_prefix("0x")
                    .unwrap_or_else(|| panic!("invalid hex: {:?}", s));
                Ok(Self::Definition(u8::from_str_radix(s, 16)?))
            }
            '.' | '@' => match s.chars().count() {
                Self::ASCII_WIDTH => {
                    let mut pos = 0;
                    for c in s.chars() {
                        pos <<= 1;
                        match c {
                            '.' => {}
                            '@' => pos += 1,
                            c => return Err(LineError::UnexpectedChar(c)),
                        }
                    }
                    Ok(Self::Body(pos))
                }
                _ => Err(LineError::InvalidLengthBody(s.into())),
            },
            c => Err(LineError::UnexpectedChar(c)),
        }
    }
}

fn load_fonts() -> Result<()> {
    let out_dir = env::var("OUT_DIR")?;
    let out_path = Path::new(&out_dir).join(OUT);
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&out_path)?;

    let src = OpenOptions::new().read(true).open(SRC)?;
    let mut lines = BufReader::new(src).lines();

    let mut remains = HashSet::new();
    for c in u8::MIN..=u8::MAX {
        let _ = remains.insert(c);
    }

    writeln!(
        &mut out,
        "// This is auto generated module and do not modify."
    )?;
    writeln!(&mut out, "#[allow(dead_code)]")?;
    writeln!(
        &mut out,
        "pub(crate) const FONT_H: usize = {};",
        Line::ASCII_HEIGHT
    )?;
    writeln!(
        &mut out,
        "pub(crate) const FONT_W: usize = {};",
        Line::ASCII_WIDTH
    )?;

    writeln!(
        &mut out,
        "pub(crate) const ASCII_FONT: [[u8; {}]; 256] = [",
        Line::ASCII_HEIGHT
    )?;

    while let Some(line) = lines.next() {
        let line = line?;
        match Line::from_str(&line)? {
            Some(Line::Definition(c)) => {
                if !remains.remove(&c) {
                    panic!("duplicating font definition for {:#02x} found", c);
                }
                let mut font = [0; Line::ASCII_HEIGHT];
                for f in font.iter_mut() {
                    let line = lines.next().expect("insufficient lines provided.")?;
                    match Line::from_str(&line)? {
                        Some(Line::Body(layout)) => *f = layout,
                        l => panic!("unexpected line: {:?}", l),
                    }
                }

                // ensure font written in binary style
                writeln!(&mut out, "\t// {:#08x}", c)?;
                writeln!(&mut out, "\t[")?;
                for &layout in font.iter() {
                    writeln!(&mut out, "\t\t{:#010b},", layout)?;
                }
                writeln!(&mut out, "\t],")?;
            }
            Some(l) => panic!("invalid format found near: {:?}", l),
            None => continue,
        }
    }
    assert!(remains.is_empty());
    writeln!(&mut out, "];")?;
    Ok(())
}

const ASM_S: &str = "asm/asm.s";
const ASM_O: &str = "asm.o";
const LIBASM: &str = "libasm.a";

fn build_asm() -> Result<()> {
    let out_dir = env::var("OUT_DIR")?;
    let out_path = Path::new(&out_dir).join(ASM_O).display().to_string();

    let status = Command::new("nasm")
        .args(&["-f", "elf64", "-o", &out_path, ASM_S])
        .status()?;
    assert!(status.success());

    let status = Command::new("ar")
        .args(&["crus", LIBASM, ASM_O])
        .current_dir(&out_dir)
        .status()?;
    assert!(status.success());

    println!("cargo:rustc-link-search={}", out_dir);
    println!("cargo:rustc-link-lib=static=asm");
    println!("cargo:rerun-if-changed={}", ASM_S);
    Ok(())
}

fn main() -> Result<()> {
    load_fonts()?;
    build_asm()
}
