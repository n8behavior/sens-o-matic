//! The credential of a device. A device signs its MLS messages with its identity
//! key, the key that names it in the logs, and the identity of its basic
//! credential is that key: a leaf of the MLS tree is a device of the group log
//! and nothing else (the rule of D6, "a user is a set of device keys", is then a
//! comparison of two sets; see [`crate::cipher`]).

use e2e_harness::{
    cipher::CipherError,
    keys::{DeviceKey, Identity},
};
use openmls::prelude::{BasicCredential, Credential, CredentialWithKey};
use openmls_traits::{
    signatures::{Signer, SignerError},
    types::SignatureScheme,
};

/// Signs with the identity key of the device.
#[derive(Debug, Clone)]
pub struct DeviceSigner(pub Identity);

impl Signer for DeviceSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(self.0.sign(payload).as_bytes().to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

/// The credential of `key`, with `key` as its signature key.
pub fn credential_of(key: &DeviceKey) -> CredentialWithKey {
    CredentialWithKey {
        credential: BasicCredential::new(key.as_bytes().to_vec()).into(),
        signature_key: key.as_bytes().to_vec().into(),
    }
}

/// The device that a credential names, or an error when it names no device.
pub fn device_key_of(credential: &Credential) -> Result<DeviceKey, CipherError> {
    let basic = BasicCredential::try_from(credential.clone())
        .map_err(|error| CipherError::Invalid(format!("the credential is not basic: {error}")))?;
    let bytes: [u8; 32] = basic.identity().try_into().map_err(|_| {
        CipherError::Invalid("the identity of a credential is not a device key".into())
    })?;
    Ok(DeviceKey::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use e2e_harness::rng::{Seed, SimRng};

    use super::*;

    #[test]
    fn a_credential_names_its_device_and_a_stranger_credential_names_none() {
        let key = Identity::generate(&mut SimRng::new(Seed(1))).device_key();
        assert_eq!(device_key_of(&credential_of(&key).credential), Ok(key));
        let stranger: Credential = BasicCredential::new(b"alice".to_vec()).into();
        assert!(device_key_of(&stranger).is_err());
    }
}
