use crate::errors::ServiceError;
use argonautica::{Hasher, Verifier};
use ethsign::{keyfile::KeyFile, protected::Protected};
use serde_json::from_reader;
use std::env;
use std::fs::File;

lazy_static::lazy_static! {
    pub  static ref SECRET_KEY: String = std::env::var("SECRET_KEY").unwrap_or_else(|_| "0123".repeat(8));

    // get private key from keystore
    pub static ref PRIVATE_KEY: String = {
        let args: Vec<String> = env::args().collect();
        assert_eq!(args.len(), 2, "Please run: cargo run [keystore's password]");
        let pwd = args[1].to_string();
        let file = File::open("./test.json").unwrap();
        let key: KeyFile = from_reader(file).unwrap();
        let password: Protected = pwd.into();
        let secret = key.to_secret_key(&password).unwrap();
        String::from("0x") + &secret.unprotected()
    };
}

// WARNING THIS IS ONLY FOR DEMO PLEASE DO MORE RESEARCH FOR PRODUCTION USE
pub fn hash_password(password: &str) -> Result<String, ServiceError> {
    println!("{:?}", password);
    Hasher::default()
        .with_password(password)
        .with_salt("test_salt")
        .with_secret_key(SECRET_KEY.as_str())
        .hash()
        .map_err(|err| {
            dbg!(err);
            ServiceError::InternalServerError
        })
}

pub fn verify(hash: &str, password: &str) -> Result<bool, ServiceError> {
    Verifier::default()
        .with_hash(hash)
        .with_password(password)
        .with_secret_key(SECRET_KEY.as_str())
        .verify()
        .map_err(|err| {
            dbg!(err);
            ServiceError::Unauthorized
        })
}
