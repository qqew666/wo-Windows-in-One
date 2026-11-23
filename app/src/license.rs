use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use sha2::Sha256;
use std::sync::OnceLock;

const DAY: i64 = 86_400;
pub const MOSCOW: i64 = 10_800;
const PUBLIC_KEY: &str = include_str!("../license-public.pem");
static VERIFIER: OnceLock<Option<VerifyingKey<Sha256>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyError {
    Format,
    Signature,
    Future,
    Expired,
}

impl KeyError {
    #[cfg(windows)]
    pub fn message(self) -> &'static str {
        match self {
            Self::Format | Self::Signature => "Ключ не распознан. Скопируйте его целиком с сайта.",
            Self::Future => "Срок действия ещё не начался. Проверьте системные часы.",
            Self::Expired => "Ключ истёк. Получите новый на сайте.",
        }
    }
}

pub fn verify(key: &str, now: i64) -> Result<i64, KeyError> {
    let key = key.trim();
    if key.len() > 512 {
        return Err(KeyError::Format);
    }
    let (payload, signed) = key.rsplit_once('.').ok_or(KeyError::Format)?;
    let mut fields = payload.split('.');
    if fields.next() != Some("TWI1") {
        return Err(KeyError::Format);
    }
    let mut timestamp = || {
        let field = fields.next().ok_or(KeyError::Format)?;
        if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(KeyError::Format);
        }
        field.parse::<i64>().map_err(|_| KeyError::Format)
    };
    let starts = timestamp()?;
    let ends = timestamp()?;
    if fields.next().is_some()
        || ends.checked_sub(starts) != Some(DAY)
        || ends > 253_402_300_799
        || starts.rem_euclid(DAY) != DAY - MOSCOW
        || signed.len() != 342
    {
        return Err(KeyError::Format);
    }

    let bytes = URL_SAFE_NO_PAD
        .decode(signed)
        .map_err(|_| KeyError::Format)?;
    let signature = Signature::try_from(bytes.as_slice()).map_err(|_| KeyError::Format)?;
    let verifier = VERIFIER.get_or_init(|| {
        RsaPublicKey::from_public_key_pem(PUBLIC_KEY)
            .ok()
            .map(VerifyingKey::new)
    });
    verifier
        .as_ref()
        .ok_or(KeyError::Signature)?
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| KeyError::Signature)?;

    if now < starts {
        Err(KeyError::Future)
    } else if now >= ends {
        Err(KeyError::Expired)
    } else {
        Ok(ends)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_product_and_bad_time_window() {
        let signature = "A".repeat(342);
        assert_eq!(verify("ignored", 0), Err(KeyError::Format));
        assert_eq!(
            verify(&format!("TWI1.75600.162001.{signature}"), 75600),
            Err(KeyError::Format)
        );
        assert_eq!(
            verify(&format!("TWI1.75600.162000.{signature}"), 75600),
            Err(KeyError::Signature)
        );
    }

    #[test]
    fn accepts_key_from_the_service_when_a_fixture_is_provided() {
        let Ok(key) = std::env::var("TWI_TEST_KEY") else {
            return;
        };
        let then = key.split('.').nth(1).unwrap().parse::<i64>().unwrap();
        assert_eq!(verify(&key, then), Ok(then + DAY));
        assert_eq!(verify(&key, then - 1), Err(KeyError::Future));
        assert_eq!(verify(&key, then + DAY - 1), Ok(then + DAY));
        assert_eq!(verify(&key, then + DAY), Err(KeyError::Expired));
    }
}
