use std::{
    fs,
    sync::{atomic::AtomicBool, Arc},
};

use clap::{command, Parser};
use colored::Colorize;
use directories::ProjectDirs;
use signal_hook::flag;

struct App {
    data_dir: String,
    args: Args,
}

// Constant Vector for VLC Locations
const VLC_LOCATIONS: [&str; 9] = [
    "/Applications/VLC.app/Contents/MacOS/VLC",
    "/Applications/VLC.app/Contents/MacOS/lib/vlc",
    "/Applications/VLC.app/Contents/MacOS/lib/vlc/vlc",
    // Windows
    "C:\\Program Files\\VideoLAN\\VLC\\vlc.exe",
    "C:\\Program Files (x86)\\VideoLAN\\VLC\\vlc.exe",
    // Linux
    "/usr/bin/vlc",
    "/usr/local/bin/vlc",
    "/snap/bin/vlc",
    "/var/lib/snapd/snap/bin/vlc",
];

// Also implement Display for StructStation

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct StructStation {
    changeuuid: String,
    stationuuid: String,
    name: String,
    url: String,
    url_resolved: String,
    homepage: String,
    favicon: String,
    tags: String,
    country: String,
    countrycode: String,
    state: String,
    language: String,
    languagecodes: String,
    votes: i32,
    lastchangetime: String,
    lastchangetime_iso8601: String,
    codec: String,
    bitrate: i32,
    hls: i8,
    lastcheckok: i8,
    lastchecktime: String,
    lastchecktime_iso8601: String,
    lastlocalchecktime: Option<String>,
    lastlocalchecktime_iso8601: Option<String>,
    lastcheckoktime: String,
    lastcheckoktime_iso8601: Option<String>,
    clicktimestamp: String,
    clicktimestamp_iso8601: String,
    clickcount: i32,
    clicktrend: i32,
    ssl_error: i32,
    geo_lat: Option<f64>,
    geo_long: Option<f64>,
    has_extended_info: bool,
}

impl std::fmt::Display for StructStation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}]", self.name.trim(), self.country)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Country {
    name: String,
    iso_3166_1: String,
    stationcount: i32,
}

impl std::fmt::Display for Country {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} ({} total stations)",
            self.iso_3166_1, self.name, self.stationcount
        )
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List countries
    #[arg(long)]
    countries: bool,

    /// Filter by country code (e.g. US)
    #[arg(short, long)]
    country: Option<String>,

    /// Filter by language code (e.g. en)
    #[arg(short, long)]
    language: Option<String>,

    /// Volume (default: 10)
    #[arg(short, long, default_value = "10")]
    volume: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if VLC is installed
    let vlc_location = VLC_LOCATIONS
        .iter()
        .find(|&&location| fs::metadata(location).is_ok());

    if vlc_location.is_none() {
        println!("Error: VLC is not installed");
        return Ok(());
    }

    let term = Arc::new(AtomicBool::new(false));

    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;

    let args = Args::parse();

    let app = App {
        data_dir: get_data_dir(),
        args,
    };

    if app.data_dir == "null" {
        println!("Error: Could not find data directory");
        return Ok(());
    }

    get_db(&app.data_dir).await?;

    println!("Hello, world!");

    let station_list = tokio::fs::read_to_string(format!("{}/stations.db", app.data_dir)).await?;

    let station_list: Vec<StructStation> = serde_json::from_str(&station_list)?;

    // Replace every blank name with "Unknown"
    let station_list = station_list
        .into_iter()
        .map(|mut station| {
            if station.name == "" {
                station.name = String::from("Unknown");
            }
            station
        })
        .collect::<Vec<StructStation>>();

    // Filter by country code

    let station_list = if let Some(country) = app.args.country {
        station_list
            .into_iter()
            .filter(|station| station.countrycode == country)
            .collect::<Vec<StructStation>>()
    } else {
        station_list
    };

    let mut country_code = String::new();

    if app.args.countries {
        let countries =
            tokio::fs::read_to_string(format!("{}/countries.json", app.data_dir)).await?;

        let countries: Vec<Country> = serde_json::from_str(&countries)?;

        println!("Country count: {}", countries.len());

        let selection =
            dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Select a country, or type to search")
                .items(&countries)
                .interact()?;

        country_code = countries[selection].iso_3166_1.clone();
    }

    // Filter by language code

    let station_list = station_list
        .into_iter()
        .filter(|station| station.countrycode == country_code)
        .collect::<Vec<StructStation>>();

    println!("Station count: {}", station_list.len());

    if station_list.len() > 100 {
        println!(
            "{} - Station count is excessively large! Fuzzy searching will be very slow.",
            "WARNING".yellow()
        );

        // Press enter to continue
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    }

    let station_selection =
        dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt("Select a station, or type to search")
            .items(&station_list)
            .interact()?;

    println!("Selected station: {}", station_list[station_selection]);

    println!(
        "Attempting to connect to {}...",
        station_list[station_selection].url
    );

    let mut vlc_command = tokio::process::Command::new(vlc_location.unwrap())
        .arg("-I")
        .arg("dummy")
        .arg("--dummy-quiet")
        .arg("--volume")
        .arg(app.args.volume.to_string())
        .arg(&station_list[station_selection].url)
        .spawn()?;

    let vlc_pid = vlc_command.id().unwrap() as i32;

    ctrlc::set_handler(move || {
        if vlc_pid != -1 {
            println!("Killing VLC... {}", vlc_pid);

            kill_process(vlc_pid);
        }
    })?;

    tokio::spawn(async move {
        while !term.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Kill VLC if it's not running

            if vlc_pid != -1 {
                kill_process(vlc_pid);
            }
        }
    });

    vlc_command.wait().await?;

    println!("Exited VLC");

    Ok(())
}

async fn get_db(data_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Create the data directory recursively if it doesn't exist
    tokio::fs::create_dir_all(&data_dir).await?;

    let mut headers = reqwest::header::HeaderMap::new();

    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static("@kalkafox/EchoTune/0.1"),
    );

    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    // Check if stations.db already exists
    let db_path = format!("{}/stations.db", data_dir);

    if !tokio::fs::metadata(&db_path).await.is_ok() {
        let db_res = client
            .get("http://all.api.radio-browser.info/json/stations")
            .send()
            .await?;

        if db_res.status().is_success() {
            let mut db_file = tokio::fs::File::create(&db_path).await?;

            let db_bytes = db_res.bytes().await?;

            tokio::io::copy(&mut &*db_bytes, &mut db_file).await?;
        }
    }

    // Check if countries.json already exists

    let countries_path = format!("{}/countries.json", data_dir);

    if !tokio::fs::metadata(&countries_path).await.is_ok() {
        let countries_res = client
            .get("http://all.api.radio-browser.info/json/countries")
            .send()
            .await?;

        if countries_res.status().is_success() {
            let mut countries_file = tokio::fs::File::create(&countries_path).await?;

            let countries_bytes = countries_res.bytes().await?;

            tokio::io::copy(&mut &*countries_bytes, &mut countries_file).await?;
        }
    }

    Ok(())
}

fn get_data_dir() -> String {
    if let Some(proj_dirs) = ProjectDirs::from("dev", "kalkafox", "EchoTune") {
        proj_dirs.data_dir().to_str().unwrap().to_string()
    } else {
        String::from("null")
    }
}

fn kill_process(pid: i32) {
    // Windows kill
    #[cfg(target_os = "windows")]
    unsafe {
        winapi::um::processthreadsapi::TerminateProcess(
            winapi::um::processthreadsapi::OpenProcess(
                winapi::um::winnt::PROCESS_TERMINATE,
                0,
                pid as u32,
            ),
            0,
        );
    }

    // Linux kill
    #[cfg(target_os = "linux")]
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    // Mac kill
    #[cfg(target_os = "macos")]
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
}
