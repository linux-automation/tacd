// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2026 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this library; if not, see <https://www.gnu.org/licenses/>.

use std::fs::{File, Permissions, create_dir, rename};
use std::io::{Cursor, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::CapabilitySet;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, Key, KeyInit},
};
use base64::Engine;
use serde::Serialize;

mod utils;
use utils::*;

#[cfg(feature = "demo_mode")]
const KEY_PATH: &str = "demo_files/srv/tacd/token_store.key";

#[cfg(not(feature = "demo_mode"))]
const KEY_PATH: &str = "/srv/tacd/token_store.key";

const FALLBACK_VALIDITY: Duration = Duration::from_mins(15);

#[derive(PartialEq, Eq, Debug, Clone, Serialize)]
pub struct Token {
    #[serde(serialize_with = "serialize_system_time")]
    pub generated: SystemTime,
    #[serde(serialize_with = "serialize_system_time")]
    pub expires: SystemTime,
    pub capabilities_read: CapabilitySet,
    pub capabilities_write: CapabilitySet,
}

pub struct TokenStore {
    cipher: Aes256Gcm,
}

#[derive(Debug)]
#[allow(unused)]
pub enum DeserializationError {
    Base64(base64::DecodeError),
    Aead(aes_gcm::Error),
    Version(u8),
    TooFewBytes,
    UnusedBytes,
}

fn gen_key(path: &Path) -> std::io::Result<()> {
    let key = Key::<Aes256Gcm>::generate();

    let parent = path.parent().unwrap();

    if !parent.exists() {
        create_dir(parent)?;
    }

    let path_tmp = {
        let mut path_tmp = path.to_owned();
        assert!(path_tmp.set_extension("key.tmp"));
        path_tmp
    };

    {
        let mut fd = File::create(&path_tmp)?;
        fd.set_permissions(Permissions::from_mode(0o400))?;
        fd.sync_all()?;

        fd.write_all(&key)?;
        fd.sync_all()?;
    }

    rename(path_tmp, path)
}

fn read_key() -> std::io::Result<Key<Aes256Gcm>> {
    let path = Path::new(KEY_PATH);

    if !path.is_file() {
        gen_key(path)?;
    }

    let key_bytes = {
        let mut buffer = [0u8; 32];

        let mut fd = File::open(path)?;
        fd.read_exact(&mut buffer)?;

        buffer
    };

    Ok(key_bytes.into())
}

impl Token {
    pub fn new(validity: Duration) -> Self {
        let now = SystemTime::now();

        Self {
            generated: whole_second(now),
            expires: whole_second(now + validity),
            capabilities_read: CapabilitySet::empty(),
            capabilities_write: CapabilitySet::empty(),
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut dst = Vec::new();

        let generated_ts = to_unix_ts(self.generated);
        let validity_in_seconds = self
            .expires
            .duration_since(self.generated)
            .unwrap()
            .as_secs()
            .min(u32::MAX as u64) as u32;

        write_u8(&mut dst, 1); // version
        write_u64(&mut dst, generated_ts);
        write_u32(&mut dst, validity_in_seconds);
        write_u16(&mut dst, self.capabilities_read.bits());
        write_u16(&mut dst, self.capabilities_write.bits());

        dst
    }

    fn deserialize(buf: &[u8]) -> Result<Token, DeserializationError> {
        let mut src = Cursor::new(buf);

        let version = read_u8(&mut src)?;

        match version {
            1 => {
                let generated_ts = read_u64(&mut src)?;
                let validity_in_seconds = read_u32(&mut src)?;
                let capability_bits_read = read_u16(&mut src)?;
                let capability_bits_write = read_u16(&mut src)?;

                read_eof(&mut src)?;

                let generated = from_unix_ts(generated_ts);
                let expires = generated + Duration::from_secs(validity_in_seconds as u64);
                let capabilities_read = CapabilitySet::from_bits(capability_bits_read);
                let capabilities_write = CapabilitySet::from_bits(capability_bits_write);

                Ok(Token {
                    generated,
                    expires,
                    capabilities_read,
                    capabilities_write,
                })
            }
            v => Err(DeserializationError::Version(v)),
        }
    }
}

impl TokenStore {
    pub fn open() -> std::io::Result<Self> {
        let key = read_key()?;
        let cipher = Aes256Gcm::new(&key);

        Ok(Self { cipher })
    }

    pub fn fallback_token(&self, is_browser: bool, is_same_origin: bool) -> Token {
        let mut token = Token::new(FALLBACK_VALIDITY);

        if !is_browser || is_same_origin {
            // Set all capabilities for requests that do not come from a browser,
            // or for requests that come from a browser but have the CSRF headers
            // set correctly.
            token.capabilities_read.set_all();
            token.capabilities_write.set_all();
        }

        token
    }

    pub fn encrypt(&self, token: &Token) -> String {
        let plaintext = token.serialize();

        let nonce = Nonce::generate();
        let mut ciphertext = self.cipher.encrypt(&nonce, plaintext.as_slice()).unwrap();

        // Prepend nonce to the ciphertext
        ciphertext.splice(0..0, nonce.iter().copied());

        base64::engine::general_purpose::URL_SAFE.encode(&ciphertext)
    }

    pub fn decrypt(&self, data: &str) -> Result<Token, DeserializationError> {
        let nonce_and_ciphertext = base64::engine::general_purpose::URL_SAFE
            .decode(data)
            .map_err(DeserializationError::Base64)?;

        let nonce = nonce_and_ciphertext
            .get(..12)
            .ok_or(DeserializationError::TooFewBytes)?
            .try_into()
            .map_err(|_| DeserializationError::TooFewBytes)?;
        let ciphertext = nonce_and_ciphertext
            .get(12..)
            .ok_or(DeserializationError::TooFewBytes)?;

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(DeserializationError::Aead)?;

        Token::deserialize(&plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_token() -> Token {
        Token::new(Duration::from_mins(15))
    }

    #[test]
    fn token_to_wire_and_back_roundtrip() {
        let token = gen_token();

        let roundtripped = Token::deserialize(&token.serialize()).unwrap();

        assert_eq!(roundtripped, token);
    }

    #[test]
    fn store_deserialize_rejects_garbage_base64() {
        let store = TokenStore::open().unwrap();
        assert!(matches!(
            store.decrypt("not valid base64 !!!"),
            Err(DeserializationError::Base64(_))
        ));
    }

    #[test]
    fn store_deserialize_rejects_too_short_input() {
        let store = TokenStore::open().unwrap();
        let data = base64::engine::general_purpose::URL_SAFE.encode([0u8; 11]);
        assert!(matches!(
            store.decrypt(&data),
            Err(DeserializationError::TooFewBytes)
        ));
    }

    #[test]
    fn store_deserialize_rejects_wrong_key() {
        let store = TokenStore::open().unwrap();
        let other_store = TokenStore::open().unwrap();
        let token = gen_token();

        assert!(matches!(
            other_store.decrypt(&store.encrypt(&token)),
            Err(DeserializationError::Aead(_))
        ));
    }

    #[test]
    fn store_deserialize_rejects_tampered_ciphertext() {
        let store = TokenStore::open().unwrap();
        let token = gen_token();

        let data = store.encrypt(&token);
        let mut bytes = base64::engine::general_purpose::URL_SAFE
            .decode(&data)
            .unwrap();
        bytes[12] ^= 0xff; // flip a bit in the ciphertext, not the nonce
        let tampered = base64::engine::general_purpose::URL_SAFE.encode(&bytes);

        assert!(matches!(
            store.decrypt(&tampered),
            Err(DeserializationError::Aead(_))
        ));
    }

    #[test]
    fn token_to_serialize_and_back_roundtrip() {
        let store = TokenStore::open().unwrap();
        let token = gen_token();

        let serialized = store.encrypt(&token);
        let roundtripped = store.decrypt(&serialized).unwrap();

        assert_eq!(roundtripped, token);
    }
}
