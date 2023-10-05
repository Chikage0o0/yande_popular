use std::{fs, path::Path, sync::OnceLock};

use anyhow::Result;
use matrix_sdk::{
    self, attachment::AttachmentConfig, config::SyncSettings, room::Joined,
    ruma::events::room::message::RoomMessageEventContent, Client,
};

use tokio::runtime::Runtime;
use url::Url;

use crate::args;

static ROOM: OnceLock<Joined> = OnceLock::new();

async fn upload(file_path: &Path) -> Result<()> {
    let image = fs::read(file_path).expect("Can't read image");
    let filename = file_path.file_name().unwrap().to_str().unwrap();
    let mime = mime_guess::from_path(file_path).first_or_octet_stream();

    // get image width and height
    let size = match image::image_dimensions(file_path) {
        Ok(size) => {
            let (width, height) = size;
            // scale image
            let scale = 1200.0 / width.max(height) as f32;
            let width = (width as f32 * scale) as u32;
            let height = (height as f32 * scale) as u32;
            Some((width, height))
        }
        Err(_) => None,
    };

    ROOM.get_or_init(init)
        .send_attachment(
            filename,
            &mime,
            &image,
            AttachmentConfig::new().generate_thumbnail(size),
        )
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

    client
        .login_username(username, password)
        .initial_device_display_name("rust-sdk")
        .send()
        .await?;

    client.sync_once(SyncSettings::new()).await?;

    let room = client
        .get_joined_room(room_id.try_into()?)
        .ok_or(anyhow::Error::msg("No room with that alias exists"))?;

    Ok(room)
}

fn init() -> Joined {
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
