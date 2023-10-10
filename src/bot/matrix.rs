use std::{fs, path::Path, sync::OnceLock};

use anyhow::Result;
use image::GenericImageView;
use matrix_sdk::{
    self, attachment::AttachmentConfig, config::SyncSettings, room::Joined,
    ruma::events::room::message::RoomMessageEventContent, Client,
};

use mime_guess::mime;
use tokio::runtime::Runtime;
use url::Url;

use crate::args;

pub static ROOM: OnceLock<Joined> = OnceLock::new();

async fn upload(file_path: &Path) -> Result<()> {
    let file = fs::read(file_path)?;
    let filename = file_path
        .file_name()
        .unwrap_or(std::ffi::OsStr::new("image.jpg"))
        .to_str()
        .unwrap_or("image.jpg");
    let mime = mime_guess::from_path(file_path).first_or_octet_stream();

    let config = match mime.type_() {
        mime::IMAGE => {
            // 从文件Bytes获取图片信息
            let image = image::load_from_memory(&file)?;
            let (width, height) = image.dimensions();
            let blurhash = blurhash::encode(4, 3, width, height, image.to_rgba8().as_raw())?;

            let info = matrix_sdk::attachment::BaseImageInfo {
                height: Some(height.try_into()?),
                width: Some(width.try_into()?),
                size: Some(file.len().try_into()?),
                blurhash: Some(blurhash),
            };

            AttachmentConfig::new().info(matrix_sdk::attachment::AttachmentInfo::Image(info))
        }
        _ => AttachmentConfig::default(),
    };

    ROOM.get_or_init(init)
        .send_attachment(filename, &mime, &file, config)
        .await?;
    Ok(())
}

pub async fn send_msg(msg: &str) -> Result<()> {
    let msg = RoomMessageEventContent::text_markdown(msg);
    ROOM.get_or_init(init).send(msg, None).await?;
    Ok(())
}

async fn login(
    homeserver_url: &str,
    username: &str,
    password: &str,
    room_id: &str,
) -> Result<Joined> {
    let homeserver_url = Url::parse(homeserver_url).expect("Couldn't parse the homeserver URL");
    let client = Client::new(homeserver_url).await.unwrap();
    let session_file = std::path::PathBuf::from(&args().data_dir).join("session");

    if session_file.exists() && restore_login(&client, &session_file).await.is_ok() {
        log::info!("Restored login from session file");
    } else {
        login_username(&client, &session_file, username, password).await?;
        log::info!("Logged in as {}", username);
    }

    client.sync_once(SyncSettings::new()).await?;

    let room = client
        .get_joined_room(room_id.try_into()?)
        .ok_or(anyhow::Error::msg("No room with that alias exists"))?;

    Ok(room)
}

async fn restore_login(client: &Client, session_file: impl AsRef<Path>) -> Result<()> {
    let session = fs::read_to_string(session_file)?;
    let session = serde_json::from_str(&session)?;
    client.restore_login(session).await?;
    Ok(())
}

async fn login_username(
    client: &Client,
    session_file: impl AsRef<Path>,
    username: &str,
    password: &str,
) -> Result<()> {
    client
        .login_username(username, password)
        .initial_device_display_name("yande_popular_bot")
        .send()
        .await?;
    let session = client.session();
    if let Some(session) = session {
        let session = serde_json::to_string(&session)?;
        fs::write(session_file, session)?;
    };
    Ok(())
}

pub fn init() -> Joined {
    std::thread::spawn(|| {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let args = args();

            login(
                &args.home_server_url,
                &args.user,
                &args.password,
                &args.room_id,
            )
            .await
            .unwrap()
        })
    })
    .join()
    .unwrap()
}

pub async fn send_attachment(file_path: &Path) -> Result<()> {
    upload(file_path).await?;
    Ok(())
}
