use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::ssh_key::LineEnding;
use russh::keys::{HashAlg, PrivateKey, PublicKey};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceKey {
    pub public_key: String,
    pub fingerprint: String,
    pub install_command: String,
}

pub fn generate(comment: &str) -> Result<PrivateKey, String> {
    let mut seed = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut seed)
        .map_err(|_| "no system random source".to_string())?;
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    key.set_comment(comment);
    Ok(key)
}

pub fn to_pem(key: &PrivateKey) -> Result<String, String> {
    key.to_openssh(LineEnding::LF)
        .map(|s| s.to_string())
        .map_err(|e| format!("cannot encode the device key: {e}"))
}

pub fn from_pem(pem: &str) -> Result<PrivateKey, String> {
    PrivateKey::from_openssh(pem).map_err(|e| format!("cannot read the device key: {e}"))
}

pub fn public_line(key: &PublicKey) -> String {
    key.to_openssh().unwrap_or_default()
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn install_command(public_line: &str) -> String {
    let quoted = public_line.replace('\'', "");
    format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && (grep -qxF '{quoted}' ~/.ssh/authorized_keys || echo '{quoted}' >> ~/.ssh/authorized_keys)"
    )
}

pub fn describe(key: &PrivateKey) -> DeviceKey {
    let line = public_line(key.public_key());
    DeviceKey {
        install_command: install_command(&line),
        fingerprint: fingerprint(key.public_key()),
        public_key: line,
    }
}

pub fn comment() -> String {
    let os = if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        std::env::consts::OS
    };
    format!("overseer-{os}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_distinct_ed25519_keys_that_round_trip() {
        let a = generate("overseer-test").unwrap();
        let b = generate("overseer-test").unwrap();
        assert_ne!(a.public_key(), b.public_key());
        assert_eq!(a.algorithm(), russh::keys::Algorithm::Ed25519);
        let pem = to_pem(&a).unwrap();
        assert!(
            pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
            "{pem}"
        );
        let back = from_pem(&pem).unwrap();
        assert_eq!(back.public_key(), a.public_key());
        assert_eq!(back.comment().as_bytes(), b"overseer-test");
        assert!(from_pem("not a key").is_err());
    }

    #[test]
    fn describes_the_public_half_for_authorized_keys() {
        let k = generate("overseer-android").unwrap();
        let d = describe(&k);
        assert!(
            d.public_key.starts_with("ssh-ed25519 AAAA")
                && d.public_key.ends_with(" overseer-android"),
            "{}",
            d.public_key
        );
        assert!(d.fingerprint.starts_with("SHA256:"), "{}", d.fingerprint);
        assert_eq!(
            PublicKey::from_openssh(&d.public_key).unwrap(),
            *k.public_key()
        );
        assert!(d.install_command.contains(&format!("'{}'", d.public_key)));
        assert!(d.install_command.contains(">> ~/.ssh/authorized_keys"));
        assert!(d.install_command.contains("grep -qxF"));
        assert!(!pem_leaks(&d));
    }

    fn pem_leaks(d: &DeviceKey) -> bool {
        d.public_key.contains("PRIVATE") || d.install_command.contains("PRIVATE")
    }

    #[test]
    fn install_command_cannot_be_broken_by_quotes() {
        let c = install_command("ssh-ed25519 AAAA x'; rm -rf ~; echo '");
        assert_eq!(c.matches('\'').count(), 4, "{c}");
    }
}
