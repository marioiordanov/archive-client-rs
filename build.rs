fn main() {

    dotenvy::dotenv().ok();

    let client_id = dotenvy::var("CLIENT_ID").unwrap();
    let client_secret = dotenvy::var("CLIENT_SECRET").unwrap();
    println!("cargo::rustc-env=CLIENT_ID={client_id}");
    println!("cargo::rustc-env=CLIENT_SECRET={client_secret}");
}
