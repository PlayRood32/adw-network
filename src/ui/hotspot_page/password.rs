// File: password.rs
// Location: /src/ui/hotspot_page/password.rs

use gtk4::prelude::*;
use rand::seq::SliceRandom;
use rand::RngExt;

use super::{MAX_PASSWORD_LEN, MIN_PASSWORD_LEN};

pub(super) fn update_strength_indicator(
    password: &str,
    label: &gtk4::Label,
    bar: &gtk4::ProgressBar,
) {
    let len = password.chars().count();
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;

    for ch in password.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else {
            has_symbol = true;
        }
    }

    let variety = has_lower as u8 + has_upper as u8 + has_digit as u8 + has_symbol as u8;
    let mut pool_size = 0usize;
    if has_lower {
        pool_size += 26;
    }
    if has_upper {
        pool_size += 26;
    }
    if has_digit {
        pool_size += 10;
    }
    if has_symbol {
        pool_size += 32;
    }

    let entropy = if pool_size == 0 || len == 0 {
        0.0
    } else {
        (len as f64) * (pool_size as f64).log2()
    };

    let (text, class) = if len > MAX_PASSWORD_LEN {
        ("Too long", "strength-weak")
    } else if len < MIN_PASSWORD_LEN || variety <= 1 {
        ("Weak (Low Entropy)", "strength-weak")
    } else if variety < 4 {
        ("Medium (Moderate Entropy)", "strength-medium")
    } else if len >= 16 {
        ("Very Strong (High Entropy)", "strength-very-strong")
    } else if len >= 12 {
        ("Strong (High Entropy)", "strength-strong")
    } else {
        ("Medium (Moderate Entropy)", "strength-medium")
    };

    bar.remove_css_class("strength-weak");
    bar.remove_css_class("strength-medium");
    bar.remove_css_class("strength-strong");
    bar.remove_css_class("strength-very-strong");
    bar.add_css_class(class);

    let fraction = if entropy <= 0.0 || len > MAX_PASSWORD_LEN {
        0.0
    } else {
        (entropy / 80.0).min(1.0)
    };

    label.set_text(text);
    bar.set_fraction(fraction);
}

pub(super) fn generate_password(len: usize, include_symbols: bool) -> String {
    const LOWER: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
    const UPPER: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const DIGITS: &[u8; 10] = b"0123456789";
    const SYMBOLS: &[u8; 23] = b"!@#$%^&*()-_=+[]{}:,.?/";

    let target_len = len.clamp(MIN_PASSWORD_LEN, MAX_PASSWORD_LEN);
    let mut rng = rand::rng();

    let mut classes: Vec<&[u8]> = vec![LOWER, UPPER, DIGITS];
    if include_symbols {
        classes.push(SYMBOLS);
    }

    let mut out: Vec<u8> = Vec::with_capacity(target_len);
    for class in &classes {
        out.push(class[rng.random_range(0..class.len())]);
    }

    let mut pool = Vec::new();
    for class in &classes {
        pool.extend_from_slice(class);
    }

    while out.len() < target_len {
        out.push(pool[rng.random_range(0..pool.len())]);
    }

    out.shuffle(&mut rng);
    out.into_iter().map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use super::generate_password;

    #[test]
    fn generated_password_respects_bounds() {
        let short = generate_password(4, false);
        assert!(short.len() >= 8);
        assert!(short.len() <= 63);

        let long = generate_password(100, true);
        assert_eq!(long.len(), 63);
    }

    #[test]
    fn generated_password_contains_expected_classes() {
        let value = generate_password(20, true);
        assert!(value.chars().any(|c| c.is_ascii_lowercase()));
        assert!(value.chars().any(|c| c.is_ascii_uppercase()));
        assert!(value.chars().any(|c| c.is_ascii_digit()));
        assert!(value.chars().any(|c| !c.is_ascii_alphanumeric()));
    }
}
