use sha2::{Digest, Sha256};

pub(crate) fn hash_value(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn normalize_phone(value: &str) -> String {
    value.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

pub(crate) fn mask_phone(value: &str) -> String {
    if value.len() >= 7 {
        let prefix = &value[..3];
        let suffix = &value[value.len().saturating_sub(4)..];
        format!("{prefix}****{suffix}")
    } else {
        "****".to_string()
    }
}

pub(crate) fn mask_email(value: &str) -> String {
    if let Some((name, domain)) = value.split_once('@') {
        if name.len() <= 2 {
            return format!("{}***@{}", &name[..1], domain);
        }

        let prefix = &name[..1];
        let suffix = &name[name.len().saturating_sub(1)..];
        format!("{prefix}***{suffix}@{domain}")
    } else {
        "***".to_string()
    }
}
