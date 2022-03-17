
use actix_web::{
    self,
    web::{self, Json},
    App, Error, HttpResponse, HttpServer, Responder,
};
use serde::Deserialize;
use specialscout_db::game::{FormIngest, sim_score};
use sqlx::{
    Acquire,
    query,
    sqlite::{SqliteConnectOptions, SqlitePool},
    ConnectOptions, Connection, Executor, pool::PoolConnection,
};
use std::{
    fs, io,
    time::{SystemTime, UNIX_EPOCH}, cmp::max
};

const IP: &str = "0.0.0.0:80";
const DB_FILE: &str = "db.sqlite";

struct AppState {
    db: SqlitePool,
}

struct TeamDetails {
    team: i32,
    matches: i32,
    taxi: i32,
    taxi_true: i32,
    preload: i32,
    auto_shoot: i32,
    auto_shoot_true: i32,
    shots_accum: i32,
    shots_upper_accum: i32,
    shots_lower_accum: i32,
    climb: i32,
    stated_climb: i32,
    score_accum: i32
}

async fn heartbeat() -> impl Responder {
    HttpResponse::Ok().body(format!("specialscout-db v{}", env!("CARGO_PKG_VERSION")))
}

#[derive(Deserialize)]
struct ResponseDump {
    responses: Vec<FormIngest>,
}

fn sqlx_to_actix(format_string: &str, error: sqlx::Error) -> HttpResponse {
    HttpResponse::RequestTimeout().body(format!("{}\nError: {:?}", format_string, error))
}

#[actix_web::post("/dump_resps/{uuid}")]
async fn dump_responses(
    web::Path((uuid,)): web::Path<(u32,)>,
    dump: Json<FormIngest>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let conn = data.db.acquire().await;

    // sqlx::Error does not convert happily to actix_web's Error
    if let Err(e) = conn {
        return Err(sqlx_to_actix("Timed out connecting to DB", e).into());
        // return Err
    }

    let mut conn = conn.unwrap();

    if let Err(e) = insert_response(&dump.0, uuid, &mut conn).await {
        Err(HttpResponse::InternalServerError().body(e.to_string()).into())
    } else {
        Ok(HttpResponse::NoContent().finish())
    }
}

async fn insert_response(
    ingest: &FormIngest,
    uuid: u32,
    db: &mut PoolConnection<sqlx::Sqlite>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = &mut *db.acquire().await?;

    match ingest {
        FormIngest::Match {
            timestamp,
            event,
            match_number,
            team_number,
            did_preload,
            did_taxi,
            got_field_cargo,
            did_hp_shot,
            did_hp_sink,
            auto_scored_lower,
            auto_scored_upper,
            auto_shots,
            teleop_scored_lower,
            teleop_scored_upper,
            teleop_shots,
            pins,
            times_pinned,
            penalties,
            performance,
            comments,
            red_score,
            blue_score,
            climb,
        } => {
            query("INSERT INTO match_responses VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(timestamp)
                .bind(uuid)
                .bind(event)
                .bind(match_number)
                .bind(team_number)
                .bind(did_preload)
                .bind(did_taxi)
                .bind(got_field_cargo)
                .bind(did_hp_shot)
                .bind(did_hp_sink)
                .bind(auto_scored_lower)
                .bind(auto_scored_upper)
                .bind(auto_shots)
                .bind(teleop_scored_lower)
                .bind(teleop_scored_upper)
                .bind(teleop_shots)
                .bind(pins)
                .bind(times_pinned)
                .bind(penalties)
                .bind(performance)
                .bind(red_score)
                .bind(blue_score)
                .bind(climb)
                .bind(comments)
                .execute(conn.acquire().await?)
                .await?;
            
            let maybe_old = query!(r#"SELECT * FROM team_details WHERE team = ?1"#, team_number).fetch_optional(&mut *conn).await?;

            if let Some(mut details) = maybe_old {
                details.matches += 1;
                details.taxi_true |= *did_taxi as i64;
                details.preload |= *did_preload as i64;
                details.auto_shoot |= (*auto_shots > 0) as i64;
                details.auto_shoot_true |= (*auto_shots > 0) as i64;
                details.auto_upper_accum += *auto_scored_upper as i64;
                details.auto_lower_accum += *auto_scored_lower as i64;
                details.shots_accum += *teleop_shots as i64;
                details.shots_upper_accum += *teleop_scored_upper as i64;
                details.shots_lower_accum += *teleop_scored_lower as i64;
                details.climb = max(details.climb, (*climb).into());
                details.score_accum += sim_score(*did_taxi, *auto_scored_upper, *auto_scored_lower, *teleop_scored_upper, *teleop_scored_lower, *climb);
                query!(r#"INSERT OR REPLACE INTO team_details VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"#,
                    team_number,
                    details.matches,
                    details.taxi,
                    details.taxi_true,
                    details.preload,
                    details.auto_shoot,
                    details.auto_shoot_true,
                    details.auto_upper_accum,
                    details.auto_lower_accum,
                    details.shots_accum,
                    details.shots_upper_accum,
                    details.shots_lower_accum,
                    details.climb,
                    details.stated_climb,
                    details.score_accum
                ).execute(conn.acquire().await?)
                .await?;
            } else {
                // initialize team details
                let matches = 1i64;
                let taxi_true = *did_taxi as i64;
                let preload = *did_preload as i64;
                let auto_shoot = (*auto_shots > 0) as i64;
                let auto_shoot_true = (*auto_shots > 0) as i64;
                let auto_upper_accum = *auto_scored_upper as i64;
                let auto_lower_accum = *auto_scored_lower as i64;
                let shots_accum = *teleop_shots as i64;
                let shots_upper_accum = *teleop_scored_upper as i64;
                let shots_lower_accum = *teleop_scored_lower as i64;
                let score_accum = sim_score(*did_taxi, *auto_scored_upper, *auto_scored_lower, *teleop_scored_upper, *teleop_scored_lower, *climb);
                query!(r#"INSERT OR REPLACE INTO team_details VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"#,
                    team_number,
                    matches,
                    taxi_true,
                    taxi_true,
                    preload,
                    auto_shoot,
                    auto_shoot_true,
                    auto_upper_accum,
                    auto_lower_accum,
                    shots_accum,
                    shots_upper_accum,
                    shots_lower_accum,
                    climb,
                    climb,
                    score_accum
                ).execute(conn.acquire().await?)
                .await?;
            }

    
            Ok(())
        }
        FormIngest::Pit {
            time_stamp,
            team_name,
            team_number,
            drivetrain,
            weight,
            size,
            climb,
            comment,
            build_quality,
            can_shoot_auto_upper,
            can_shoot_auto_lower,
            can_shoot_teleop_upper,
            can_shoot_teleop_lower,
            driver_team,
            confidence,
            picture,
        } => {
            query("INSERT INTO pit_responses VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(time_stamp)
                .bind(uuid)
                .bind(team_number)
                .bind(team_name)
                .bind(drivetrain)
                .bind(weight)
                .bind(size.x)
                .bind(size.y)
                .bind(size.z)
                .bind(can_shoot_auto_lower)
                .bind(can_shoot_auto_upper)
                .bind(can_shoot_teleop_lower)
                .bind(can_shoot_teleop_upper)
                .bind(climb)
                .bind(build_quality)
                .bind(confidence)
                .bind(driver_team)
                .bind(comment)
                .bind(picture)
                .execute(&mut *conn)
                .await?;
            
            let maybe_old = query!(r#"SELECT * FROM team_details WHERE team = ?1"#, team_number).fetch_optional(&mut *conn).await?;

            if let None = maybe_old {
                let team = team_number;  
                let matches: i32 = 0;
                let taxi: i32 = 0;
                let taxi_true: i32 = 0;
                let preload: i32 = 0;
                let auto_shoot: i32 = (can_shoot_auto_lower | can_shoot_auto_upper) as i32;
                let auto_shoot_true: i32 = 0;
                let auto_upper_accum: i32 = 0;
                let auto_lower_accum: i32 = 0;
                let shots_accum: i32 = 0;
                let shots_upper_accum: i32 = 0;
                let shots_lower_accum: i32 = 0;
                let climb: i32 = 0;
                let stated_climb: i32 = climb;
                let score_accum: i32 = 0;
                query!(r#"INSERT OR REPLACE INTO team_details VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"#,
                    team,
                    matches,
                    taxi,
                    taxi_true,
                    preload,
                    auto_shoot,
                    auto_shoot_true,
                    auto_upper_accum,
                    auto_lower_accum,
                    shots_accum,
                    shots_upper_accum,
                    shots_lower_accum,
                    climb,
                    stated_climb,
                    score_accum
                ).execute(conn.acquire().await?)
                .await?;
            } 

            query!(r#"INSERT OR REPLACE INTO images VALUES (?1, ?2)"#, team_number, picture).execute(conn).await?;

            Ok(())
        },
    }
}

#[actix_web::post("/dump_resps_mass/{uuid}")]
async fn dump_responses_mass(
    web::Path((uuid,)): web::Path<(u32,)>,
    dump: Json<ResponseDump>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let conn = data.db.acquire().await;

    // sqlx::Error does not convert happily to actix_web's Error
    if let Err(e) = conn {
        return Ok(sqlx_to_actix("Timed out connecting to DB", e));
        // return Err
    }

    let mut conn = conn.unwrap();

    for ingest in &dump.responses {
        if let Err(e) = insert_response(ingest, uuid, &mut conn).await {
            return Ok(HttpResponse::InternalServerError().body(e.to_string()));
        }
    }

    Ok(HttpResponse::NoContent().finish())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Hello, world!");

    let initial_conn_options = SqliteConnectOptions::new()
        .filename(DB_FILE)
        .create_if_missing(false);
    // let conn = SqliteConnection::connect(&format!("sqlite:{}", DB_FILE)).await;
    let conn = initial_conn_options.connect().await;

    if let Err(e) = conn {
        println!(
            "Ran into error {:?} while connecting to db! Assuming db is foobar, recreating...",
            e
        );

        if fs::metadata(DB_FILE).is_ok() {
            if fs::metadata(DB_FILE).is_ok() {
                if let Err(e) = fs::copy(
                    DB_FILE,
                    format!(
                        "{}_{}bak",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis(),
                        DB_FILE
                    ),
                ) {
                    println!("Could not create a backup of the DB file!");
                    println!("{:?}", e);
                    println!("Clear {}? > ", DB_FILE);
                    let mut buffer = String::new();
                    io::stdin()
                        .read_line(&mut buffer)
                        .expect("Could not read stdin, exiting to avoid clearing DB.");

                    if !buffer.to_lowercase().contains("y") {
                        println!("Please manually create a backup of {} and rerun.", DB_FILE);
                        return Ok(());
                    }
                }
                fs::remove_file(DB_FILE).expect(&format!(
                    "Could not clear existing db file! Ensure you have write access to {}",
                    DB_FILE
                ))
            }
            println!("Creating new {}...", DB_FILE);
        } else {
            println!("Could not write to this directory. Check permissions.");
            return Ok(());
        }

        let initial_conn_options = SqliteConnectOptions::new()
            .filename(DB_FILE)
            .create_if_missing(true);
        let mut conn = initial_conn_options
            .connect()
            .await
            .expect("Could not create db file!");

        conn.execute(
            r#"CREATE TABLE "uuids" (
            "uuid"	INTEGER NOT NULL UNIQUE,
            "name"	TEXT,
            "team_number"	INTEGER,
            PRIMARY KEY("uuid")
        )"#,
        )
        .await
        .expect("Failed creating uuids table!");

        conn.execute(
            r#"CREATE TABLE "teams" (
                "team_number"	INTEGER NOT NULL,
                "matches_played"	INTEGER NOT NULL DEFAULT 0,
                "scouts"	INTEGER DEFAULT 0,
                "responses"	INTEGER DEFAULT 0,
                PRIMARY KEY("team_number")
            );"#,
        )
        .await
        .expect("Failed creating teams table!");

        conn.execute(
            r#"CREATE TABLE "matches" (
                "event"	TEXT NOT NULL DEFAULT 'Unknown',
                "match_number"	INTEGER,
                "group"	TEXT,
                "teams"	TEXT,
                PRIMARY KEY("event")
            )"#,
        )
        .await
        .expect("Failed creating matches table!");

        conn.execute(
            r#"CREATE TABLE "match_responses" (
                "timestamp"	INTEGER NOT NULL,
                "uuid" INTEGER NOT NULL,
                "event"	TEXT NOT NULL,
                "team_number" INTEGER NOT NULL,
                "match_number"	INTEGER NOT NULL,
                "did_preload"	INTEGER NOT NULL,
                "did_taxi"	INTEGER NOT NULL,
                "got_field_cargo"	INTEGER NOT NULL,
                "did_hp_shot"   INTEGER NOT NULL,
                "did_hp_sink"   INTEGER NOT NULL,
                "auto_scored_lower"	INTEGER NOT NULL,
                "auto_scored_upper"	INTEGER NOT NULL,
                "auto_shots"	INTEGER NOT NULL,
                "teleop_scored_lower"	INTEGER NOT NULL,
                "teleop_scored_upper"	INTEGER NOT NULL,
                "teleop_shots"	INTEGER NOT NULL,
                "pins"	INTEGER NOT NULL,
                "times_pinned"	INTEGER NOT NULL,
                "penalties"	INTEGER NOT NULL,
                "performance" INTEGER NOT NULL,
                "red_score" INTEGER NOT NULL,
                "blue_score" INTEGER NOT NULL,
                "climb" INTEGER NOT NULL,
                "comment"	TEXT NOT NULL
            );"#,
        )
        .await
        .expect("Failed creating match responses table!");
        
        conn.execute(
            r#"CREATE TABLE "pit_responses" (
                "timestamp"	INTEGER NOT NULL,
                "uuid" INTEGER NOT NULL,
                "team"	INTEGER NOT NULL,
                "team_name"	TEXT NOT NULL,
                "weight"	INTEGER NOT NULL,
                "drivetrain"    TEXT NOT NULL,
                "size_x"	INTEGER NOT NULL,
                "size_y"	INTEGER NOT NULL,
                "size_z"	INTEGER NOT NULL,
                "can_shoot_auto_upper"	INTEGER NOT NULL,
                "can_shoot_auto_lower"	INTEGER NOT NULL,
                "can_shoot_teleop_upper"	INTEGER NOT NULL,
                "can_shoot_teleop_lower"	INTEGER NOT NULL,
                "climb"	INTEGER NOT NULL,
                "build_quality"	INTEGER NOT NULL,
                "confidence"	INTEGER NOT NULL,
                "driver_team"	INTEGER NOT NULL,
                "comment"	TEXT NOT NULL,
                "image"	BLOB
            );"#,
        )
        .await
        .expect("Failed creating pit responses table!");

        conn.execute(r#"CREATE TABLE "team_details" (
            "team"	INTEGER NOT NULL UNIQUE,
            "matches"	INTEGER NOT NULL,
            "taxi"	INTEGER NOT NULL,
            "taxi_true"	INTEGER NOT NULL,
            "preload"	INTEGER NOT NULL,
            "auto_shoot"	INTEGER NOT NULL,
            "auto_shoot_true"	INTEGER NOT NULL,
            "auto_upper_accum"	INTEGER NOT NULL DEFAULT 0,
            "auto_lower_accum"	INTEGER NOT NULL DEFAULT 0,
            "shots_accum"	INTEGER NOT NULL DEFAULT 0,
            "shots_upper_accum"	INTEGER NOT NULL DEFAULT 0,
            "shots_lower_accum"	INTEGER NOT NULL DEFAULT 0,
            "climb"	INTEGER NOT NULL DEFAULT 0,
            "stated_climb"	INTEGER NOT NULL DEFAULT 0,
            "score_accum"	INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("team")
        )"#).await.expect("Failed creating team details table!");

        conn.execute(r#"CREATE TABLE "images" (
            "team"	INTEGER NOT NULL UNIQUE,
            "img"	BLOB,
            PRIMARY KEY("team")
        )"#).await.expect("Failed creating images table!");

        conn.close();

        println!("Finished creating new db {}", DB_FILE);
    } else {
        drop(conn);
    }

    println!("Starting specialscout v{}...", env!("CARGO_PKG_VERSION"));

    let pool = create_pool().await;

    HttpServer::new(move || {
        App::new()
            .service(dump_responses)
            .route("/heartbeat", web::get().to(heartbeat))
            .data(AppState { db: pool.clone() })
    })
    .bind(IP)?
    .run()
    .await
}

async fn create_pool() -> SqlitePool {
    SqlitePool::connect(&format!("sqlite:{}", DB_FILE))
        .await
        .expect("Failed to create pool!")
}
