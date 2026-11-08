//! Immutable, version-selected CPython scalar tables without private C imports.
use pyo3::{exceptions::PyRuntimeError, prelude::*};

struct Tables {
    unicode: &'static str,
    flags: &'static [u8],
    lower: &'static [u8],
}

macro_rules! table {
    ($minor:literal, $version:literal) => {
        Tables {
            unicode: $version,
            flags: include_bytes!(concat!("host_unicode_data/", $minor, "-flags.bin")),
            lower: include_bytes!(concat!("host_unicode_data/", $minor, "-lower.bin")),
        }
    };
}

// bounded: five supported CPython Unicode databases, independent of RDF size.
static TABLES: [Tables; 5] = [
    table!("11", "14.0.0"),
    table!("12", "15.0.0"),
    table!("13", "15.1.0"),
    table!("14", "16.0.0"),
    table!("15", "17.0.0"),
];

fn tables() -> &'static Tables {
    // SAFETY: Py_Version is immutable process data in the stable ABI since 3.11.
    let version = unsafe { pyo3::ffi::Py_Version };
    assert_eq!(version >> 24, 3, "Unsupported Python major version");
    let minor = ((version >> 16) & 255) as usize;
    TABLES
        .get(minor.wrapping_sub(11))
        .expect("Unsupported Python Unicode authority")
}

pub(crate) fn validate(py: Python<'_>) -> PyResult<()> {
    let actual: String = py
        .import("unicodedata")?
        .getattr("unidata_version")?
        .extract()?;
    if actual != tables().unicode {
        return Err(PyRuntimeError::new_err("Unsupported host Unicode database"));
    }
    Ok(())
}

fn flags(ch: char) -> u8 {
    let scalar = ch as usize;
    let data = if scalar < 128 {
        TABLES[0].flags
    } else {
        tables().flags
    };
    let offset = (scalar >> 8) * 2;
    let block = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
    data[8704 + block * 256 + (scalar & 255)]
}

/// Match the selected host's decimal scalar table without objects or a GIL.
pub fn decimal(ch: char) -> Option<u8> {
    if ch.is_ascii() {
        return ch.is_ascii_digit().then(|| ch as u8 - b'0');
    }
    (flags(ch) & 15).checked_sub(1)
}

/// Match Python alphanumeric-or-underscore word membership.
pub fn word(ch: char) -> bool {
    flags(ch) & 16 != 0
}

/// Match Python whitespace membership, including its control separators.
pub fn whitespace(ch: char) -> bool {
    flags(ch) & 32 != 0
}

/// Match Python regex simple lowercase, rather than Unicode full lowercase.
pub fn simple_lower(ch: char) -> char {
    if ch.is_ascii() {
        return ch.to_ascii_lowercase();
    }
    let data = tables().lower;
    let scalar = ch as u32;
    // Most of the Unicode domain lies above the final nonidentity mapping.
    // Its exact answer needs no binary search, including during policy setup.
    let last = data.len() - 8;
    let maximum = u32::from_le_bytes(data[last..last + 4].try_into().unwrap());
    if scalar > maximum {
        return ch;
    }
    let mut start = 0;
    let mut end = data.len() / 8;
    while start < end {
        let middle = start + (end - start) / 2;
        let offset = middle * 8;
        let key = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        match scalar.cmp(&key) {
            std::cmp::Ordering::Less => end = middle,
            std::cmp::Ordering::Greater => start = middle + 1,
            std::cmp::Ordering::Equal => {
                let value = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap());
                return char::from_u32(value).expect("Generated lowercase must be a scalar");
            }
        }
    }
    ch
}
