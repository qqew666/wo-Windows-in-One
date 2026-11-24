use semver::Version;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

pub const SITE: &str = "https://two-windows.sillypetunia8.chatgpt.site/";
const REPO: &str = "https://github.com/qqew666/wo-Windows-in-One";
const RELEASE_API: &str = "https://api.github.com/repos/qqew666/wo-Windows-in-One/releases/latest";
const SETUP: &str = "Two-Windows-in-One-Setup.exe";
const MAX_INSTALLER: u64 = 32 * 1024 * 1024;

fn agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(4)
            .timeout_global(Some(Duration::from_secs(seconds)))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build(),
    )
}

pub fn short_code(value: &str) -> Option<String> {
    let code: String = value
        .chars()
        .filter(|ch| *ch != '-' && !ch.is_ascii_whitespace())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    (code.len() == 10
        && code
            .bytes()
            .all(|ch| b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ".contains(&ch)))
    .then_some(code)
}

pub fn activate(code: &str) -> Result<String, String> {
    let mut response = agent(15)
        .post(format!("{SITE}api/activate"))
        .send_json(json!({ "code": code }))
        .map_err(|error| match error {
            ureq::Error::StatusCode(422) => {
                "Ключ неверный или истёк. Получите актуальный ключ на сайте.".to_owned()
            }
            _ => format!("Сервис активации недоступен ({error})."),
        })?;
    let payload: Value = response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_json()
        .map_err(|error| format!("Сервис вернул неверный ответ ({error})."))?;
    payload["value"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Сервис не вернул подтверждение активации.".into())
}

#[derive(Clone)]
pub struct Release {
    pub version: Version,
    tag: String,
    size: u64,
    digest: String,
}

fn parse_release(metadata: &Value, installed: &Version) -> Result<Option<Release>, String> {
    let tag = metadata["tag_name"]
        .as_str()
        .ok_or_else(|| "GitHub не вернул номер релиза.".to_owned())?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| "Номер релиза GitHub не распознан.".to_owned())?;
    if &version <= installed
        || !version.pre.is_empty()
        || metadata["prerelease"] == true
        || metadata["draft"] == true
    {
        return Ok(None);
    }

    let asset = metadata["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|asset| asset["name"].as_str() == Some(SETUP))
        .ok_or_else(|| "В релизе GitHub нет установщика.".to_owned())?;
    let size = asset["size"]
        .as_u64()
        .filter(|size| (2..=MAX_INSTALLER).contains(size))
        .ok_or_else(|| "Неверный размер установщика.".to_owned())?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|value| value.strip_prefix("sha256:"))
        .filter(|value| value.len() == 64 && value.bytes().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| "GitHub не предоставил контрольную сумму установщика.".to_owned())?;
    let expected_url = format!("{REPO}/releases/download/{tag}/{SETUP}");
    if asset["browser_download_url"].as_str() != Some(expected_url.as_str()) {
        return Err("GitHub вернул неожиданный адрес установщика.".into());
    }
    Ok(Some(Release {
        version,
        tag: tag.to_owned(),
        size,
        digest: digest.to_ascii_lowercase(),
    }))
}

pub fn check_update() -> Result<Option<Release>, String> {
    let mut response = agent(15)
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "TwoWindowsInOne-Updater")
        .call()
        .map_err(|error| format!("Не удалось проверить релизы GitHub ({error})."))?;
    let metadata: Value = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_json()
        .map_err(|error| format!("Ответ GitHub не распознан ({error})."))?;
    let installed =
        Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo validates the app version");
    parse_release(&metadata, &installed)
}

fn verify_installer(bytes: &[u8], release: &Release) -> Result<(), String> {
    if bytes.len() as u64 != release.size
        || !bytes.starts_with(b"MZ")
        || format!("{:x}", Sha256::digest(bytes)) != release.digest
    {
        return Err("Установщик не совпадает с контрольной суммой GitHub.".into());
    }
    Ok(())
}

pub fn download(release: &Release, data: &Path) -> Result<PathBuf, String> {
    let url = format!("{REPO}/releases/download/{}/{SETUP}", release.tag);
    let mut response = agent(90)
        .get(url)
        .call()
        .map_err(|error| format!("Не удалось скачать обновление ({error})."))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(release.size)
        .read_to_vec()
        .map_err(|error| format!("Загрузка обновления прервана ({error})."))?;
    verify_installer(&bytes, release)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let cache = data.join("Updates");
    let folder = cache.join(format!("{}-{stamp}", std::process::id()));
    fs::create_dir_all(cache)
        .and_then(|_| fs::create_dir(&folder))
        .map_err(|error| format!("Папку обновления создать не удалось ({error})."))?;
    let path = folder.join(SETUP);
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(folder);
        return Err(format!("Не удалось сохранить установщик ({error})."));
    }
    Ok(path)
}

pub fn check_download(path: &Path, release: &Release) -> Result<(), String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("Установщик недоступен ({error})."))?
        .len();
    if size != release.size {
        return Err("Размер скачанного установщика изменился. Загрузите его заново.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(release.size + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("Установщик не прочитан ({error})."))?;
    verify_installer(&bytes, release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_github_installer_and_validates_its_digest() {
        let bytes = b"MZinstaller";
        let metadata = json!({
            "tag_name": "v9.1.0",
            "assets": [{
                "name": SETUP,
                "size": bytes.len(),
                "digest": format!("sha256:{:x}", Sha256::digest(bytes)),
                "browser_download_url": format!("{REPO}/releases/download/v9.1.0/{SETUP}")
            }]
        });
        let current = Version::new(9, 0, 0);
        let release = parse_release(&metadata, &current).unwrap().unwrap();
        assert!(verify_installer(bytes, &release).is_ok());
        assert!(verify_installer(b"MZInstaller", &release).is_err());
        assert!(
            parse_release(&metadata, &Version::new(9, 1, 0))
                .unwrap()
                .is_none()
        );
        let mut bad = metadata;
        bad["assets"][0]["browser_download_url"] = json!("https://example.com/setup.exe");
        assert!(parse_release(&bad, &current).is_err());
    }

    #[test]
    fn normalizes_only_unambiguous_ten_character_codes() {
        assert_eq!(short_code("ABc24-def67").as_deref(), Some("ABC24DEF67"));
        assert!(short_code("ABC24-IO167").is_none());
        assert!(short_code("ABC24-DEF6").is_none());
    }
}
