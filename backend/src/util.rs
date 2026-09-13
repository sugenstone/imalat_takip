use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// UUIDv7 - zaman sirali, index dostu.
pub fn new_id() -> String {
    Uuid::now_v7().to_string()
}

/// RFC3339 UTC zaman damgasi.
pub fn now() -> String {
    Utc::now().to_rfc3339()
}

/// N gun sonra - RFC3339 (session bitisleri icin).
pub fn now_plus_days(days: i64) -> String {
    (Utc::now() + chrono::Duration::days(days)).to_rfc3339()
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Slug uretimi (Turkce karakter destekli).
pub fn slugify(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    for c in lower.chars() {
        let mapped = match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' | 'İ' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        out.push(mapped);
    }
    let slug: String = out
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "ws".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

/// Seri bolum olusturma sablon calistirici.
/// Desteklenen tokenlar: {n}, {nn}, {nnn}, {parent}
pub fn render_serial_name(template: &str, number: usize, parent_name: Option<&str>) -> String {
    let mut name = template.to_string();
    if number <= 999 {
        name = name.replace("{nnn}", &format!("{:03}", number));
        name = name.replace("{nn}", &format!("{:02}", number));
    } else {
        name = name.replace("{nnn}", &number.to_string());
        name = name.replace("{nn}", &number.to_string());
    }
    name = name.replace("{n}", &number.to_string());
    name = name.replace("{parent}", parent_name.unwrap_or(""));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_turkish() {
        assert_eq!(slugify("Ahmet'in Atölyesi"), "ahmet-in-atolyesi");
        assert_eq!(slugify("Üretim -- Alanı!"), "uretim-alani");
    }

    #[test]
    fn serial_names() {
        assert_eq!(render_serial_name("Daire {n}", 5, None), "Daire 5");
        assert_eq!(render_serial_name("Daire {nn}", 5, None), "Daire 05");
        assert_eq!(render_serial_name("{parent}-Daire-{nnn}", 7, Some("Blok A")), "Blok A-Daire-007");
    }
}
