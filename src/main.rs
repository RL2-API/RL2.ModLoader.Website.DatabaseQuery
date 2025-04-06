use axum::{ routing::get, extract::{ State, Path }, http::StatusCode, response::Redirect, Router, Json };
use libsql::{ Builder, Database };
use std::sync::{ Arc };
use std::vec::{ Vec };
use tokio::sync::{ RwLock };
use tower_http::cors::{ Any, CorsLayer };

#[tokio::main]
async fn main() {
    // Set up local DB copy
    let db = Builder::new_local("mods.db")
        .build()
        .await.expect("Failed to create local replica");

    let conn = db.connect().expect("Failed to connect to local replica");

    conn.execute("
        CREATE TABLE IF NOT EXISTS info (name VARCHAR(64), author VARCHAR(48), icon_src TEXT, short_desc VARCHAR(128), long_desc TEXT);
    ", ()).await.expect("Failed to create table 'info'");

    conn.execute("
        CREATE TABLE IF NOT EXISTS versions (id INTEGER PRIMARY KEY, name VARCHAR(64), link TEXT, version VARCHAR(32), changelog TEXT);
    ", ()).await.expect("Failed to create table 'versions'");
    
    dotenvy::dotenv().expect("Failed to load .env");
    let db_url = dotenvy::var("DB_URL").expect("Missing database URL");
    let auth = dotenvy::var("AUTH").expect("Missing auth token");
    let sync_auth = dotenvy::var("SYNC_AUTH").expect("Missing sync auth token");
    
    // Add local db to shared state
    let app_state = Arc::new(AppState { 
        db: Arc::new(RwLock::new(db)), 
        remote_url: db_url,
        auth: auth,
        sync_auth: sync_auth.clone()
    });

    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api", get(homepage))
        .route("/api/mod-list", get(mod_list))
        .route("/api/mod/{name}", get(mod_data))
        .route("/api/run-sync/{auth}", get(sync_local))
        .with_state(app_state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.expect("Failed to create a listener");
    axum::serve(listener, app.into_make_service()).await.expect("Failed to serve the app");
}

#[derive(Clone)]
struct AppState {
    db: Arc<RwLock<Database>>,
    remote_url: String,
    auth: String,
    sync_auth: String
}

async fn homepage() -> &'static str {
    "
    Routes:
        - GET /api/mod-list - returns a JSON list of all mods sorted by 'recently updated' or HTTP 500
        - GET /api/mod/{name} - returns a JSON object representing info about the mod with the specified name or HTTP 500 if some component fails, or HTTP 404 if the mod doesn't exist
    "
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModListEntry {
    name: String,
    author: String,
    icon_src: Option<String>,
    short_desc: String,
}

async fn mod_list(
    State(state): State<Arc<AppState>>
) -> Result<Json<Vec<ModListEntry>>, StatusCode> {
    let connection = match state.db.read().await.connect() {
        Ok(val) => val,
        Err(_err) => {
            println!("Connection failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut queried = match connection.query("
        SELECT info.name, info.author, info.icon_src, info.short_desc
        FROM info INNER JOIN versions ON info.name = versions.name 
        GROUP BY info.name 
        ORDER BY MAX(versions.id) DESC
    ", ()).await {
        Ok(val) => val,
        Err(_err) => {
            println!("query failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
 
    let mut mods = Vec::new();

    while let Some(row) = queried.next().await.unwrap() {
        mods.push(ModListEntry {
            name: row.get(0).unwrap(),
            author: row.get(1).unwrap(),
            icon_src: row.get(2).unwrap(),
            short_desc: row.get(3).unwrap(),
        });
    }

    Ok(Json(mods))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModEntry {
    mod_info: ModInfoData,
    versions: Vec<VersionData>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModInfoData {
    name: String,
    author: String,
    icon_src: Option<String>,
    long_desc: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VersionData {
    link: String,
    version: String,
    changelog: Option<String>,
}

async fn mod_data(
    State(state): State<Arc<AppState>>,
    Path(mod_name): Path<String>
) -> Result<Json<ModEntry>, StatusCode> {
    let connection = match state.db.read().await.connect() {
        Ok(val) => val,
        Err(_err) => {
            println!("Connection failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut mod_info_row =  match connection.query("
            SELECT name, author, icon_src, long_desc
            FROM info
            WHERE name LIKE ?1
        ", libsql::params![mod_name.clone()]).await {
        Ok(val) => val,
        Err(_err) => {
            println!("Query failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    let mod_info = match mod_info_row.next().await.unwrap() {
        Some(value) => ModInfoData { 
            name: value.get(0).unwrap(), 
            author: value.get(1).unwrap(), 
            icon_src: value.get(2).unwrap(),
            long_desc: value.get(3).unwrap(),
        },
        None => {
            println!("mod_info for {} not found", mod_name.clone());
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let mut versions_rows = match connection.query("
            SELECT link, version, changelog
            FROM versions 
            WHERE name LIKE ?1
            ORDER BY version DESC
        ", libsql::params![mod_name]).await {
            Ok(val) => val,
            Err(_err) => {
                println!("Query failed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    let mut versions = Vec::new();
    while let Some(row) = versions_rows.next().await.unwrap() {
        versions.push( VersionData{
            link: row.get(0).unwrap(),
            version: row.get(1).unwrap(),
            changelog: row.get(2).unwrap(),
        });
    }

    Ok(Json(ModEntry { mod_info, versions }))
}

async fn sync_local(
    State(state): State<Arc<AppState>>,
    Path(auth): Path<String>
) -> Redirect {
    if state.sync_auth != auth {
        println!("Unauthorized access attempted");
        return Redirect::to("/api");
    }

    let conn = state.db.read().await.connect().expect("Local connection failed");

    println!("Dropping old tables");
    conn.execute("DROP TABLE info", ()).await.expect("Drop 'info' failed");
    conn.execute("DROP TABLE versions", ()).await.expect("Drop 'verions' failed");

    println!("Creating new tables");
    conn.execute("
        CREATE TABLE IF NOT EXISTS info (name VARCHAR(64), author VARCHAR(48), icon_src TEXT, short_desc VARCHAR(128), long_desc TEXT);
    ", ()).await.expect("Failed to create table 'info'");

    conn.execute("
        CREATE TABLE IF NOT EXISTS versions (id INTEGER PRIMARY KEY, name VARCHAR(64), link TEXT, version VARCHAR(32), changelog TEXT);
    ", ()).await.expect("Failed to create table 'versions'");
    
    let remote_db = Builder::new_remote(state.remote_url.clone(), state.auth.clone())
        .build()
        .await.expect("Failed to establish a connection to the remote");
    
    let remote_conn = remote_db.connect().expect("Failed to connect to remote database");
    let mut queried_info = remote_conn.query("
        SELECT * FROM info
    ", ()).await.expect("Remote database 'info' query failed");

    let mut queried_versions = remote_conn.query("
        SELECT * FROM versions
    ", ()).await.expect("Remote database 'versions' query failed");
    

    println!("Adding data to info...");
    while let Some(row) = queried_info.next().await.expect("Malformed row") {
        conn.execute("
            INSERT INTO info VALUES (?1, ?2, ?3, ?4, ?5)
        ", (
            row.get::<String>(0).expect("name missing"),
            row.get::<String>(1).expect("author missing"),
            row.get::<Option<String>>(2).expect("icon_src missing"),
            row.get::<Option<String>>(3).expect("short_desc missing"),
            row.get::<Option<String>>(4).expect("long_desc missing")
        )).await.expect("Info insert failed");
    } 
    
    println!("Adding data to versions...");
    while let Some(row) = queried_versions.next().await.expect("Malformed row") {
        conn.execute("
            INSERT INTO versions VALUES (?1, ?2, ?3, ?4, ?5)
        ", (
            row.get::<i32>(0).expect("id missing"),
            row.get::<String>(1).expect("name missing"),
            row.get::<String>(2).expect("link missing"),
            row.get::<String>(3).expect("version missing"),
            row.get::<Option<String>>(4).expect("changelog missing")
        )).await.expect("Info insert failed");
    }
    println!("Synced!");
    Redirect::to("/api")
}
