use std::sync::{Arc, OnceLock};

use tokio::sync::Semaphore;

use clap::Parser;
use yande::DB_HANDLE;

mod bot;
mod db;
mod yande;

#[cfg(feature = "voce")]
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 发送到的频道ID
    #[arg(short, long, env = "CHANNEL_ID")]
    channel_id: String,

    /// 机器人API_KEY
    #[arg(short, long, env = "API_KEY")]
    api_key: String,

    /// 服务器域名
    #[arg(short, long, env = "SERVER_DOMAIN")]
    server_domain: String,

    /// 服务存储临时文件的目录
    /// 默认为data
    #[arg(short, long, default_value = "data", env = "DATA_DIR")]
    data_dir: String,

    /// 并发线程数
    /// 默认为4
    #[arg(short, long, default_value = "1")]
    thread: usize,
}

#[cfg(feature = "matrix")]
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Home Server URL
    #[arg(long, env = "HOME_SERVER_URL")]
    home_server_url: String,

    /// 发送到的房间ID
    #[arg(long, env = "ROOM_ID")]
    room_id: String,

    /// 机器人用户名
    #[arg(short, long, env = "USER")]
    user: String,

    /// 机器人密码
    #[arg(short, long, env = "PASSWORD")]
    password: String,

    /// 服务存储临时文件的目录
    /// 默认为data
    #[arg(short, long, default_value = "data", env = "DATA_DIR")]
    data_dir: String,

    /// 并发线程数
    /// 默认为4
    #[arg(short, long, default_value = "1")]
    thread: usize,
}

static ARGS: OnceLock<Args> = OnceLock::new();

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let tmp_dir = std::path::Path::new(&args().data_dir).join("tmp");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let ctrlc = tokio::signal::ctrl_c();
    let interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
    tokio::pin!(ctrlc);
    tokio::pin!(interval);
    loop {
        tokio::select! {
            _ = &mut ctrlc => {
                log::info!("Ctrl-C received, exiting...");
                break;
            }
            _ = interval.tick() => {
                run().await;
                DB_HANDLE.get_or_init(db::DB::init).auto_remove().unwrap();
            }
        }
    }
}

pub(crate) fn args() -> &'static Args {
    ARGS.get_or_init(Args::parse)
}

async fn run() {
    let resp = yande::get("https://yande.re/post/popular_recent")
        .await
        .unwrap();

    let mut image_list = yande::get_image_list(&resp).unwrap();

    let resp = yande::get("https://yande.re/post/popular_recent?period=1w")
        .await
        .unwrap();

    image_list.extend(yande::get_image_list(&resp).unwrap());

    let download_list = yande::get_download_list(image_list).await.unwrap();

    let semaphore = Arc::new(Semaphore::new(args().thread));
    let mut tasks = Vec::new();
    for (id, img_data) in download_list {
        let semaphore_clone = Arc::clone(&semaphore);
        tasks.push(tokio::spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();
            let msg = format!("ID：[{id}](https://yande.re/post/show/{id})");
            bot::send_msg(&msg)
                .await
                .unwrap_or_else(|e| log::error!("send msg failed: {}", e));
            for (id, url) in img_data.url.iter() {
                log::info!("prepare download: {}", id);
                let path = match yande::download_img((*id, url)).await {
                    Ok(path) => path,
                    Err(e) => {
                        log::error!("download failed: {}", e);
                        return;
                    }
                };
                log::info!("upload: {}", id);

                bot::send_attachment(&path)
                    .await
                    .unwrap_or_else(|e| log::error!("send attachment failed: {}", e));
            }
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }
}
