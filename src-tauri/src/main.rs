#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod database;
#[cfg(feature = "logo-cache")]
mod logos;
mod scanner;
mod server;
mod setup;
mod shader_catalog;
mod steam;

use std::env;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
use tracing::{error, info};

const DEFAULT_CONFIG: &str = "config/scanner_conf.yaml";
const BIND_ADDR: &str = "127.0.0.1:8086";

fn main() {
    let log_file = tracing_appender::rolling::never(".", "music_server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tauri::Builder::default()
        .setup(|_app| {
            tauri::async_runtime::spawn(init_and_serve());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                if std::net::TcpStream::connect("127.0.0.1:8086").is_ok() { break; }
                if std::time::Instant::now() >= deadline { break; }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn init_and_serve() {
    setup::load_dotenv();

    let start = Instant::now();

    if env::var("STEAM_ID").is_err() || env::var("STEAM_API_KEY").is_err() {
        info!("SETUP: STEAM_ID or STEAM_API_KEY not set — starting first-time setup");
        let creds = setup::run_setup().await;
        env::set_var("STEAM_ID", &creds.steam_id);
        env::set_var("STEAM_API_KEY", &creds.api_key);
    }

    let mut cfg = config::load_config(DEFAULT_CONFIG);
    info!("STEAM SCANNER: config loaded from {DEFAULT_CONFIG}");

    let player_db = cfg.player_db_path();
    let steam_details_db = cfg.steam_details_db_path();

    let scanning = Arc::new(AtomicBool::new(true));
    let shared_db = database::make_placeholder_db();

    let shader_catalog: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(b"[]".to_vec()));
    let shader_total = Arc::new(AtomicUsize::new(0));

    let db_for_server = shared_db.clone();
    let scan_flag = scanning.clone();
    let sdb = steam_details_db.clone();
    let extensions = cfg.extensions();

    tauri::async_runtime::spawn(server::start(
        BIND_ADDR,
        db_for_server,
        scan_flag,
        extensions.clone(),
        sdb,
        shader_catalog.clone(),
        shader_total.clone(),
    ));

    let mut conn = match database::backup_and_init(&cfg.db_file) {
        Ok(c) => c,
        Err(e) => {
            error!("ERROR: failed to init database: {e}");
            return;
        }
    };

    #[cfg(feature = "logo-cache")]
    let mut logo_appids: Vec<String> = Vec::new();

    match steam::find_steam_dir(cfg.steam_dir.as_deref()) {
        Some(steam_dir) => {
            info!("STEAM: install dir: {steam_dir:?}");
            for root in steam::steam_scan_roots(&steam_dir) {
                if !cfg.scan_roots.contains(&root) {
                    cfg.scan_roots.push(root);
                }
            }
            match steam::load_steam_libraries(&steam_dir) {
                Ok(apps) => {
                    if let Err(e) = database::insert_steam_apps(&mut conn, &apps) {
                        error!("DB: failed to insert steam apps: {e}");
                    }
                }
                Err(e) => error!("STEAM: failed to load libraries: {e}"),
            }
        }
        None => error!("STEAM: could not locate Steam installation, skipping library scan"),
    }

    match steam::owned_apps() {
        Ok(owned) => {
            if let Err(e) = database::insert_owned_apps(&mut conn, &owned) {
                error!("DB: failed to insert owned apps: {e}");
            }
            match database::open_player_db(&steam_details_db) {
                Ok(pconn) => {
                    if let Err(e) = database::sync_owned_to_player_db(&pconn, &owned) {
                        error!("DB: failed to sync to steam_details.db: {e}");
                    }
                }
                Err(e) => error!("DB: failed to open steam_details.db: {e}"),
            }
            #[cfg(feature = "logo-cache")]
            { logo_appids = owned.iter().map(|a| a.appid.clone()).collect(); }
        }
        Err(e) => error!("STEAM: skipping owned games ({e})"),
    }

    info!(
        "SCANNER: scanning for {:?} in {} roots",
        extensions,
        cfg.scan_roots.len()
    );

    let files = scanner::scan_all(&cfg.scan_roots, &extensions);
    info!("SCANNER: found {} files", files.len());

    if let Err(e) = database::insert_steam_files(&mut conn, &files) {
        error!("DB: failed to insert steam files: {e}");
    }

    drop(conn);

    if let Err(e) = database::init_player_db(&player_db) {
        error!("DB: failed to init player.db: {e}");
    }

    #[cfg(feature = "logo-cache")]
    {
        match rusqlite::Connection::open(&player_db) {
            Ok(mut pconn) => {
                if let Err(e) = database::insert_logo_cache_placeholders(&mut pconn, &logo_appids) {
                    error!("LOGOS: failed to insert placeholders: {e}");
                }
            }
            Err(e) => error!("LOGOS: failed to open player.db: {e}"),
        }
        logos::spawn_logo_loader(player_db.clone(), cfg.logo_cache_path());
    }

    info!(
        "STEAM SCANNER: scan done in {:.3}s",
        start.elapsed().as_secs_f64()
    );

    match database::open_server_db(&cfg.db_file, &player_db, &steam_details_db) {
        Ok(conn) => { *shared_db.lock().unwrap() = conn; }
        Err(e) => error!("DB: failed to open server db: {e}"),
    }

    let (catalog_json, count) = shader_catalog::load_from_db(&cfg.shader_db_path());
    *shader_catalog.lock().unwrap() = catalog_json;
    shader_total.store(count, Ordering::Relaxed);

    scanning.store(false, Ordering::Relaxed);
}
