extern crate cita_tool;
extern crate cita_types;
extern crate hex;
extern crate hmac;
extern crate sha2;

use actix_web::{error::BlockingError, web, HttpResponse};
use cita_tool::{
    client::basic::{Client, ClientExt},
    crypto::Encryption,
    PrivateKey, TransactionOptions, UnverifiedTransaction,
};
use cita_types::U256;
use diesel::prelude::*;
use diesel::PgConnection;
use futures::Future;
use hex::encode;
use hmac::{Hmac, Mac};
use serde_json::{from_str, Value};
use sha2::Sha256;
use std::str;
use std::str::FromStr;

use crate::errors::ServiceError;
use crate::models::{Pool, User};
use crate::schema::users::dsl::{email, users};
use crate::utils::PRIVATE_KEY;

type HmacSha256 = Hmac<Sha256>;

pub const STORE_ADDRESS: &str = "0xffffffffffffffffffffffffffffffffff010000";
pub const RPC_URL: &str = "http://101.132.38.100:1337";

#[derive(Debug, Deserialize)]
pub struct TransferData {
    pub email: String,
    pub timestamp: i64,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferReturnData {
    pub rescode: i64,
    pub resmsg: String,
    pub data: TxObj,
}

#[derive(Debug, Deserialize)]
pub struct UploadData {
    pub email: String,
    pub evidence: String,
    pub timestamp: i64,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxObj {
    pub txid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadReturnData {
    pub rescode: i64,
    pub resmsg: String,
    pub data: TxObj,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvidenceObj {
    pub evidence: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryReturnData {
    pub rescode: i64,
    pub resmsg: String,
    pub data: EvidenceObj,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryData {
    pub email: String,
    pub txid: String,
    pub timestamp: i64,
    pub signature: String,
}

// add new funtion: send a transaction for 1 TDT
pub fn transfer_one(
    transfer_data: web::Json<TransferData>,
    pool: web::Data<Pool>,
) -> impl Future<Item = HttpResponse, Error = ServiceError> {
    println!("i am here");
    web::block(move || query_transfer(transfer_data.into_inner(), pool)).then(
        move |res: Result<TransferReturnData, BlockingError<ServiceError>>| match res {
            Ok(transfer_return_data) => Ok(HttpResponse::Ok().json(&transfer_return_data)),
            Err(err) => match err {
                BlockingError::Error(service_error) => Err(service_error),
                BlockingError::Canceled => Err(ServiceError::InternalServerError),
            },
        },
    )
}

pub fn query_transfer(
    transfer_data: TransferData,
    pool: web::Data<Pool>,
) -> Result<TransferReturnData, ServiceError> {
    let conn: &PgConnection = &pool.get().unwrap();
    let mut items = users
        .filter(email.eq(&transfer_data.email))
        .load::<User>(conn)?;
    println!("112");
    if let Some(user) = items.pop() {
        // useless
        println!("116");
        // what does the normal transaction msg looks like
        if let Ok(matching) = verify_sig_transfer() {
            println!("119");
            if matching {
                println!("121");
                // configure the encryption method
                let encryption = Encryption::Secp256k1;
                // get private key
                let priv_key: PrivateKey = PrivateKey::from_str(PRIVATE_KEY.as_str(), encryption)
                    .unwrap()
                    .into();

                // transaction configuration
                let tx_options = TransactionOptions::new()
                    .set_code("") // msg
                    .set_address(STORE_ADDRESS) // address
                    .set_value(Some(U256::from_str("1000000000000000000000").unwrap())); // transfer amount
                // the client object for send transaction
                let client = Client::new();
                // rpc address
                let mut client = client.set_uri(RPC_URL);
                // configure private key
                let client = client.set_private_key(&priv_key);
                
                // cita-cli
                // rpc return
                let rpc_response = client.send_raw_transaction(tx_options).unwrap();

                // more specific
                let response_value = rpc_response.result().unwrap().to_string();

                // Value is a enum type, at this place, it's a Object<Map>
                let tx_obj: Value = from_str(&response_value).expect("json was not well-formatted");
                let mut tx_hash = tx_obj["hash"].to_string();

                let tx_hash_len = tx_hash.len();
                tx_hash.remove(0);
                tx_hash.remove(tx_hash_len - 2);

                let data_obj = TxObj { txid: tx_hash };
                let res = TransferReturnData {
                    rescode: 1,
                    resmsg: "Success".to_string(),
                    data: data_obj,
                };

                return Ok(res);
            }
        }
    }
    Err(ServiceError::Unauthorized)
}

pub fn upload(
    upload_data: web::Json<UploadData>,
    pool: web::Data<Pool>,
) -> impl Future<Item = HttpResponse, Error = ServiceError> {
    web::block(move || query_diesel(upload_data.into_inner(), pool)).then(
        move |res: Result<UploadReturnData, BlockingError<ServiceError>>| match res {
            Ok(return_data) => Ok(HttpResponse::Ok().json(&return_data)),
            Err(err) => match err {
                BlockingError::Error(service_error) => Err(service_error),
                BlockingError::Canceled => Err(ServiceError::InternalServerError),
            },
        },
    )
}

/// Diesel query
pub fn query_diesel(
    upload_data: UploadData,
    pool: web::Data<Pool>,
) -> Result<UploadReturnData, ServiceError> {
    let conn: &PgConnection = &pool.get().unwrap();
    let mut items = users
        .filter(email.eq(&upload_data.email))
        .load::<User>(conn)?;

    if let Some(user) = items.pop() {
        let mut msg: String = String::from("evidence=");
        msg = msg + &upload_data.evidence + "&timestamp=" + &upload_data.timestamp.to_string();

        if let Ok(matching) = verify_sig(&user.hash, &msg, &upload_data.signature) {
            if matching {
                let msg_hex_str = encode(msg);
                let mut msg_hex_string = String::from("0x");
                msg_hex_string += &msg_hex_str; //code

                let encryption = Encryption::Secp256k1;
                let priv_key: PrivateKey = PrivateKey::from_str(PRIVATE_KEY.as_str(), encryption)
                    .unwrap()
                    .into();

                let tx_options = TransactionOptions::new()
                    .set_code(&msg_hex_string)
                    .set_address(STORE_ADDRESS)
                    .set_value(Some(U256::from_str("0").unwrap()));
                let client = Client::new();
                let mut client = client.set_uri(RPC_URL);
                let client = client.set_private_key(&priv_key);

                let rpc_response = client.send_raw_transaction(tx_options).unwrap();
                let response_value = rpc_response.result().unwrap().to_string();
                let tx_obj: Value = from_str(&response_value).expect("json was not well-formatted");
                let mut tx_hash = tx_obj["hash"].to_string();

                let tx_hash_len = tx_hash.len();
                tx_hash.remove(0);
                tx_hash.remove(tx_hash_len - 2);

                let data_obj = TxObj { txid: tx_hash };
                let res = UploadReturnData {
                    rescode: 1,
                    resmsg: "Success".to_string(),
                    data: data_obj,
                };

                return Ok(res);
            }
        }
    }
    Err(ServiceError::Unauthorized)
}

fn verify_sig_transfer() -> Result<bool, ServiceError> {
    Ok(true)
}

fn verify_sig(key: &str, msg: &str, sig: &str) -> Result<bool, ServiceError> {
    let mut mac =
        HmacSha256::new_varkey(key.to_string().as_bytes()).expect("Expect corrent format.");

    mac.input(msg.to_string().as_bytes());

    let result = mac.result();
    let code_bytes = result.code();
    let sig_get = encode(code_bytes);

    if sig_get != sig {
        println!(
            "Failed to vertify the hmac signature.\n",
        );
        Err(ServiceError::Unauthorized)
    } else {
        Ok(true)
    }
}

pub fn query(
    query_data: web::Json<QueryData>,
    pool: web::Data<Pool>,
) -> impl Future<Item = HttpResponse, Error = ServiceError> {
    web::block(move || query_chain(query_data.into_inner(), pool)).then(
        move |res: Result<QueryReturnData, BlockingError<ServiceError>>| match res {
            Ok(res) => Ok(HttpResponse::Ok().json(&res)),
            Err(err) => match err {
                BlockingError::Error(service_error) => Err(service_error),
                BlockingError::Canceled => Err(ServiceError::InternalServerError),
            },
        },
    )
}

pub fn query_chain(
    query_data: QueryData,
    pool: web::Data<Pool>,
) -> Result<QueryReturnData, ServiceError> {
    let conn: &PgConnection = &pool.get().unwrap();
    let mut items = users
        .filter(email.eq(&query_data.email))
        .load::<User>(conn)?;

    if let Some(user) = items.pop() {
        let mut msg: String = String::from("txid=");
        msg = msg + &query_data.txid + "&timestamp=" + &query_data.timestamp.to_string();

        if let Ok(matching) = verify_sig(&user.hash, &msg, &query_data.signature) {
            if matching {
                let txid = query_data.txid;
                // cita-cli rpc getTransaction --hash 0x6ca4004ec71b3a1e83fb566b7a8d7f992c86e3df1d41748da924a595f17e8312 --url http://101.132.38.100:1337

                let client = Client::new();
                let client = client.set_uri(RPC_URL);
                let rpc_response = client.get_transaction(&txid).unwrap();
                let response_value = rpc_response.result().unwrap().to_string();

                let tx_obj: Value = from_str(&response_value).expect("json was not well-formatted");
                let mut tx_content = tx_obj["content"].to_string();

                let content_len = tx_content.len();
                tx_content.remove(0);
                tx_content.remove(content_len - 2);

                let tx = UnverifiedTransaction::from_str(&tx_content).unwrap();
                let tx = match tx.transaction.as_ref() {
                    Some(tx) => tx,
                    None => return Err(ServiceError::Unauthorized),
                };
                let mut tx_content = str::from_utf8(&tx.data).unwrap().to_string();
                let offset = tx_content.find('&').unwrap_or(tx_content.len());

                let text: String = tx_content.drain(9..offset).collect();
                let data_obj = EvidenceObj { evidence: text };
                let res: QueryReturnData = QueryReturnData {
                    rescode: 1,
                    resmsg: "Success".to_string(),
                    data: data_obj,
                };

                return Ok(res);
            }
        }
    }
    Err(ServiceError::Unauthorized)
}
