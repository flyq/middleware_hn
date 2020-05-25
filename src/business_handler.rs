extern crate hex;
extern crate hmac;
extern crate sha2;

use actix_web::{error::BlockingError, web, HttpResponse};
use diesel::prelude::*;
use diesel::PgConnection;
use futures::Future;
use hex::{decode, encode};
use hmac::{Hmac, Mac};
use serde_json::{from_str, Value};
use sha2::Sha256;
use std::process::Command;
use std::str;

use crate::errors::ServiceError;
use crate::models::{Pool, User};
use crate::schema::users::dsl::{email, users};
use crate::utils::PRIVATE_KEY;

type HmacSha256 = Hmac<Sha256>;

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
                msg_hex_string += &msg_hex_str;

                let store_tx_command = Command::new("cita-cli")
                    .args(&["store", "data", "--content"])
                    .arg(&msg_hex_string)
                    .arg("--private-key")
                    .arg(PRIVATE_KEY.as_str())
                    .args(&["--url", "http://101.132.38.100:1337"])
                    .output()
                    .expect("failed to construct store trasaction");

                let command_out = store_tx_command.stdout;
                let tx_out = str::from_utf8(&command_out).unwrap();
                let tx_obj: Value = from_str(tx_out).expect("json was not well-formatted");
                let mut tx_hash = tx_obj["result"]["hash"].to_string();
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

fn verify_sig(key: &str, msg: &str, sig: &str) -> Result<bool, ServiceError> {
    let mut mac =
        HmacSha256::new_varkey(key.to_string().as_bytes()).expect("Expect corrent format.");

    mac.input(msg.to_string().as_bytes());

    let result = mac.result();
    let code_bytes = result.code();
    let sig_get = encode(code_bytes);

    if sig_get != sig {
        println!(
            " input sig: {:?}\n new sig: {:?}\n key: {:?}\n msg: {:?}",
            sig, sig_get, key, msg
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
                let store_tx_command = Command::new("cita-cli")
                    .args(&["rpc", "getTransaction", "--hash"])
                    .arg(&txid)
                    .args(&["--url", "http://101.132.38.100:1337"])
                    .output()
                    .expect("failed to construct store trasaction");

                let command_out = store_tx_command.stdout;
                let tx_out = str::from_utf8(&command_out).unwrap();
                let tx_obj: Value = from_str(tx_out).expect("json was not well-formatted");
                let mut tx_content = tx_obj["result"]["content"].to_string();
                let content_len = tx_content.len();
                tx_content.remove(0);
                tx_content.remove(content_len - 2);
                println!("content: {:?}\n", tx_content);

                let decode_content_command = Command::new("cita-cli")
                    .args(&["tx", "decode-unverifiedTransaction", "--content"])
                    .arg(&tx_content)
                    .output()
                    .expect("failed to decode content data");

                let command_out = decode_content_command.stdout;
                let tx_out = str::from_utf8(&command_out).unwrap();
                let tx_obj: Value = from_str(tx_out).expect("json was not well-formatted");
                let mut tx_content = tx_obj["transaction"]["data"].to_string();
                let content_len = tx_content.len();
                tx_content.remove(0);
                tx_content.remove(content_len - 2);
                tx_content.remove(0);
                tx_content.remove(0);
                let text_vec = decode(tx_content.as_str()).unwrap();
                let mut text = str::from_utf8(&text_vec).unwrap().to_string();
                let offset = text.find('&').unwrap_or(text.len());
                let text: String = text.drain(9..offset).collect();

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
