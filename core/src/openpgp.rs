//! Just enough OpenPGP to check a release's `SHA256SUMS.asc` against the
//! release key, so the updater trusts the key the releases are signed with,
//! not only the server that hands out the files.
//!
//! The release key (docs/hfpredict-release-key.asc) is one Ed25519 key that
//! certifies and signs, with no subkeys, and `gpg --detach-sign` makes a
//! version 4 binary signature with it (RFC 9580: public-key algorithm 22,
//! "EdDSALegacy"). That one case is all this reads; anything else is refused,
//! never guessed at. The key is compiled in and trusted as it is: its
//! self-signatures and expiry are not read (docs/releasing.md says how to
//! renew or replace it).

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256, Sha512};

/// The key every release's `SHA256SUMS` is signed with.
pub const RELEASE_KEY: &str = include_str!("../../docs/hfpredict-release-key.asc");

const TAG_SIGNATURE: u8 = 2;
const TAG_PUBLIC_KEY: u8 = 6;
const ALGO_EDDSA_LEGACY: u8 = 22;
const SIG_BINARY: u8 = 0x00;
const HASH_SHA256: u8 = 8;
const HASH_SHA512: u8 = 10;
const SUBPACKET_CREATED: u8 = 2;
/// The OID of Ed25519 in OpenPGP (1.3.6.1.4.1.11591.15.1).
const OID_ED25519: [u8; 9] = [0x2B, 0x06, 0x01, 0x04, 0x01, 0xDA, 0x47, 0x0F, 0x01];

/// Whether `asc` (an armoured detached signature) is a good signature of
/// `data` by the armoured public key `key`.
pub fn verify(key: &str, data: &[u8], asc: &str) -> bool {
    let Some(key) = dearmor(key).and_then(|k| public_key(&k)) else {
        return false;
    };
    let Some(raw) = dearmor(asc) else {
        return false;
    };
    let Some(sig) = packets(&raw) else {
        return false;
    };
    // A file may carry several signatures; one by the key is enough.
    sig.iter()
        .filter(|(tag, _)| *tag == TAG_SIGNATURE)
        .any(|(_, body)| check(&key, data, body).is_some())
}

/// The bytes inside an ASCII-armoured block. The checksum line is skipped
/// (RFC 9580 lets readers ignore it); the signature check covers it all.
fn dearmor(text: &str) -> Option<Vec<u8>> {
    let mut lines = text.lines().map(str::trim);
    lines.find(|l| l.starts_with("-----BEGIN PGP "))?;
    // Headers ("Version: …") end at the first blank line.
    for l in lines.by_ref() {
        if l.is_empty() {
            break;
        }
    }
    let mut b64 = String::new();
    for l in lines {
        if l.starts_with("-----END PGP ") || l.starts_with('=') {
            return base64(&b64);
        }
        b64.push_str(l);
    }
    None
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// The packets in a binary OpenPGP message, as (tag, body). Partial
/// lengths and indeterminate lengths are refused (neither appears in a key
/// or a detached signature).
fn packets(mut b: &[u8]) -> Option<Vec<(u8, &[u8])>> {
    let mut out = vec![];
    while !b.is_empty() {
        let ctb = b[0];
        if ctb & 0x80 == 0 {
            return None;
        }
        let (tag, len, head): (u8, usize, usize) = if ctb & 0x40 != 0 {
            let tag = ctb & 0x3F;
            match *b.get(1)? {
                o @ 0..192 => (tag, usize::from(o), 2),
                o @ 192..224 => {
                    let len = ((usize::from(o) - 192) << 8) + usize::from(*b.get(2)?) + 192;
                    (tag, len, 3)
                }
                255 => (
                    tag,
                    u32::from_be_bytes(b.get(2..6)?.try_into().ok()?) as usize,
                    6,
                ),
                _ => return None,
            }
        } else {
            let tag = (ctb >> 2) & 0x0F;
            match ctb & 3 {
                0 => (tag, usize::from(*b.get(1)?), 2),
                1 => (
                    tag,
                    usize::from(u16::from_be_bytes(b.get(1..3)?.try_into().ok()?)),
                    3,
                ),
                2 => (
                    tag,
                    u32::from_be_bytes(b.get(1..5)?.try_into().ok()?) as usize,
                    5,
                ),
                _ => return None,
            }
        };
        let body = b.get(head..head.checked_add(len)?)?;
        out.push((tag, body));
        b = &b[head + len..];
    }
    Some(out)
}

/// An MPI: its bytes, and what follows it.
fn mpi(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let bits = usize::from(u16::from_be_bytes(b.get(0..2)?.try_into().ok()?));
    let n = bits.div_ceil(8);
    let v = b.get(2..2 + n)?;
    // Exactly as many bits as the value has (RFC 9580, 3.2): one encoding
    // per value, so nothing in a signature can change unnoticed.
    let top = v.first().map_or(0, |&x| 8 - x.leading_zeros() as usize);
    if bits != 0 && (top == 0 || bits != (n - 1) * 8 + top) {
        return None;
    }
    Some((v, &b[2 + n..]))
}

/// An MPI as a fixed 32-byte value: MPIs drop leading zero bytes.
fn padded32(v: &[u8]) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    out.get_mut(32usize.checked_sub(v.len())?..)?
        .copy_from_slice(v);
    Some(out)
}

/// The first public key in an armoured key file: a version 4 Ed25519 key.
fn public_key(bytes: &[u8]) -> Option<VerifyingKey> {
    let (_, body) = packets(bytes)?
        .into_iter()
        .find(|(tag, _)| *tag == TAG_PUBLIC_KEY)?;
    // Version, creation time (4), algorithm, then the curve's OID.
    if *body.first()? != 4 || *body.get(5)? != ALGO_EDDSA_LEGACY {
        return None;
    }
    let oid_len = usize::from(*body.get(6)?);
    if body.get(7..7 + oid_len)? != OID_ED25519 {
        return None;
    }
    // The point, in native form: 0x40 then the 32-byte key.
    let (point, _) = mpi(&body[7 + oid_len..])?;
    let (&prefix, key) = point.split_first()?;
    if prefix != 0x40 {
        return None;
    }
    VerifyingKey::from_bytes(key.try_into().ok()?).ok()
}

/// Checks one version 4 signature packet over `data`.
fn check(key: &VerifyingKey, data: &[u8], sig: &[u8]) -> Option<()> {
    let [version, kind, algo, hash, ..] = *sig else {
        return None;
    };
    if version != 4 || kind != SIG_BINARY || algo != ALGO_EDDSA_LEGACY {
        return None;
    }
    let hashed_len = usize::from(u16::from_be_bytes(sig.get(4..6)?.try_into().ok()?));
    let hashed_end = 6 + hashed_len;
    let hashed = sig.get(6..hashed_end)?;
    if !subpackets_understood(hashed)? {
        return None;
    }
    let unhashed_len = usize::from(u16::from_be_bytes(
        sig.get(hashed_end..hashed_end + 2)?.try_into().ok()?,
    ));
    let rest = sig.get(hashed_end + 2 + unhashed_len..)?;
    let left16 = rest.get(0..2)?;
    let (r, rest) = mpi(&rest[2..])?;
    let (s, _) = mpi(rest)?;

    // The data, the signature's hashed part, then its trailer.
    let mut trailer = vec![4, 0xFF];
    trailer.extend_from_slice(&(hashed_end as u32).to_be_bytes());
    let digest: Vec<u8> = match hash {
        HASH_SHA256 => Sha256::new()
            .chain_update(data)
            .chain_update(&sig[..hashed_end])
            .chain_update(&trailer)
            .finalize()
            .to_vec(),
        HASH_SHA512 => Sha512::new()
            .chain_update(data)
            .chain_update(&sig[..hashed_end])
            .chain_update(&trailer)
            .finalize()
            .to_vec(),
        _ => return None,
    };
    if digest.get(0..2)? != left16 {
        return None;
    }
    let mut bytes = [0u8; 64];
    bytes[..32].copy_from_slice(&padded32(r)?);
    bytes[32..].copy_from_slice(&padded32(s)?);
    // EdDSALegacy signs the digest itself.
    key.verify_strict(&digest, &Signature::from_bytes(&bytes))
        .ok()
}

/// Whether every critical hashed subpacket is one this reader knows (RFC
/// 9580: a signature with a critical subpacket the reader does not know is
/// not valid); `None` if they do not parse.
fn subpackets_understood(mut b: &[u8]) -> Option<bool> {
    while !b.is_empty() {
        let (len, head): (usize, usize) = match b[0] {
            o @ 0..192 => (usize::from(o), 1),
            o @ 192..255 => (
                ((usize::from(o) - 192) << 8) + usize::from(*b.get(1)?) + 192,
                2,
            ),
            255 => (
                u32::from_be_bytes(b.get(1..5)?.try_into().ok()?) as usize,
                5,
            ),
        };
        let body = b.get(head..head.checked_add(len)?)?;
        let (&kind, _) = body.split_first()?;
        if kind & 0x80 != 0 && kind & 0x7F != SUBPACKET_CREATED {
            return Some(false);
        }
        b = &b[head + len..];
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // v0.2.1's SHA256SUMS and its signature, as released.
    const SUMS: &str = include_str!("../tests/data/SHA256SUMS-0.2.1");
    const ASC: &str = include_str!("../tests/data/SHA256SUMS-0.2.1.asc");

    #[test]
    fn reads_the_release_key() {
        let k = public_key(&dearmor(RELEASE_KEY).unwrap()).unwrap();
        // `gpg --list-packets` of key 4E5E D724 6337 53C0 8313 C543 5899
        // 97F4 47B7 7166: pkey[1] without its 0x40 prefix.
        let hex: String = k.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(
            hex,
            "A15AF10E057BEC29CF3003393E7B2883B6F69A9551E89EA509BE6588ACD37FBF"
        );
    }

    #[test]
    fn a_released_signature_verifies() {
        assert!(verify(RELEASE_KEY, SUMS.as_bytes(), ASC));
    }

    #[test]
    fn a_changed_file_does_not() {
        let changed = SUMS.replacen('0', "1", 1);
        assert!(!verify(RELEASE_KEY, changed.as_bytes(), ASC));
        let longer = format!("{SUMS}x");
        assert!(!verify(RELEASE_KEY, longer.as_bytes(), ASC));
    }

    #[test]
    fn a_changed_signature_does_not() {
        // Flip one bit in each byte of the signature's body in turn.
        let raw = dearmor(ASC).unwrap();
        let key = public_key(&dearmor(RELEASE_KEY).unwrap()).unwrap();
        for i in 2..raw.len() {
            let mut bad = raw.clone();
            bad[i] ^= 0x01;
            let ok = packets(&bad).is_some_and(|ps| {
                ps.iter()
                    .any(|(t, b)| *t == TAG_SIGNATURE && check(&key, SUMS.as_bytes(), b).is_some())
            });
            // Only the unhashed issuer subpacket may change unnoticed.
            let (_, body) = packets(&raw).unwrap()[0];
            let hashed_end = 6 + usize::from(u16::from_be_bytes([body[4], body[5]]));
            let unhashed =
                (2 + hashed_end + 2)..(2 + hashed_end + 2 + body[hashed_end + 1] as usize);
            assert!(
                !ok || unhashed.contains(&i),
                "byte {i} changed and still verified"
            );
        }
    }

    #[test]
    fn another_key_does_not() {
        // A fresh Ed25519 key in the same packet layout.
        let raw = dearmor(RELEASE_KEY).unwrap();
        let ps = packets(&raw).unwrap();
        let (_, body) = ps.iter().find(|(t, _)| *t == TAG_PUBLIC_KEY).unwrap();
        let mut other = body.to_vec();
        let n = other.len();
        // The basepoint: a valid point that is not the release key.
        let base = curve_basepoint();
        other[n - 32..].copy_from_slice(&base);
        let mut packet = vec![0x99, 0, other.len() as u8];
        packet.extend_from_slice(&other);
        let other_key = public_key(&packet).unwrap();
        let raw = dearmor(ASC).unwrap();
        let (_, sig) = packets(&raw).unwrap()[0];
        assert!(check(&other_key, SUMS.as_bytes(), sig).is_none());
    }

    fn curve_basepoint() -> [u8; 32] {
        let mut b = [0x66u8; 32];
        b[0] = 0x58;
        b
    }

    #[test]
    fn garbage_is_refused() {
        assert!(!verify(RELEASE_KEY, SUMS.as_bytes(), ""));
        assert!(!verify(RELEASE_KEY, SUMS.as_bytes(), "not armour"));
        assert!(!verify("", SUMS.as_bytes(), ASC));
        assert!(!verify(
            RELEASE_KEY,
            SUMS.as_bytes(),
            "-----BEGIN PGP SIGNATURE-----\n\n////\n-----END PGP SIGNATURE-----"
        ));
        assert_eq!(padded32(&[1; 33]), None);
        assert_eq!(padded32(&[1, 2]).unwrap()[30..], [1, 2]);
    }
}
