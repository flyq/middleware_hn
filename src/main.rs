#[macro_use]
extern crate diesel;
#[macro_use]
extern crate serde_derive;

use actix_web::{middleware, web, App, HttpServer};
use diesel::prelude::*;
use diesel::r2d2::{self, ConnectionManager};

mod business_handler;
mod errors;
mod invitation_handler;
mod models;
mod register_handler;
mod schema;
mod utils;

fn main() -> std::io::Result<()> {
    println!("start");
    dotenv::dotenv().ok();
    std::env::set_var("RUST_LOG", "actix_web=info,actix_server=info");
    env_logger::init();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // create db connection pool
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool: models::Pool = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");
    // let domain: String = std::env::var("DOMAIN").unwrap_or_else(|_| "localhost".to_string());

    // Start http server
    HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            // enable logger
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(4096))
            .service(
                web::scope("/v1")
                    .service(
                        web::resource("/invitation")
                            .route(web::post().to_async(invitation_handler::post_invitation)),
                    )
                    .service(
                        web::resource("/register/{invitation_id}")
                            .route(web::post().to_async(register_handler::register_user)),
                    )
                    .service(
                        web::resource("/evidence/upload")
                            .route(web::post().to_async(business_handler::upload)),
                    )
                    .service(
                        web::resource("/evidence/query")
                            .route(web::get().to_async(business_handler::query)),
                    )
                    .service(
                        web::resource("/evidence/transfer_one")
                            .route(web::post().to_async(business_handler::transfer_one)),
                    ),
            )
    })
    .bind("127.0.0.1:3000")?
    .run()
}
