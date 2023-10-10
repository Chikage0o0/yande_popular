use std::{
    fs::create_dir_all,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};

use anyhow::Result;
use tokio::sync::Semaphore;

use clap::Parser;
use yande::DB_HANDLE;

mod bot;
mod db;
mod resize;
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

    /// 服务存储文件的目录
    /// 默认为data
    #[arg(short, long, default_value = "data", env = "DATA_DIR")]
    data_dir: String,

    /// 并发线程数
    /// 默认为1
    #[arg(short, long, default_value = "1")]
    thread: usize,
}

static ARGS: OnceLock<Args> = OnceLock::new();
static STOP_SIGNAL: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    log::info!("start yande.rs bot");
    log::info!("args: {:?}", args());

    create_dir_all(&args().data_dir).unwrap();

    #[cfg(feature = "matrix")]
    {
        log::info!("test login");
        bot::matrix::ROOM.get_or_init(bot::matrix::room_init);
        tokio::spawn(bot::matrix::e2ee::sync(
            bot::matrix::CLIENT.get().unwrap().clone(),
        ));
        log::info!("login success");
    }

    let tmp_dir = std::path::Path::new(&args().data_dir).join("tmp");
    create_dir_all(&tmp_dir).unwrap();

    let ctrlc = tokio::signal::ctrl_c();
    let interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
    tokio::pin!(ctrlc);
    tokio::pin!(interval);
    while !STOP_SIGNAL.load(Ordering::Relaxed) {
        tokio::select! {
            _ = &mut ctrlc => {
                log::info!("Ctrl-C received, exiting...");
                STOP_SIGNAL.store(true, Ordering::Relaxed);
            }
            _ = interval.tick() => {
                log::info!("start scan");
                run().await.unwrap_or_else(|e| log::error!("run failed: {}", e));
                DB_HANDLE.get_or_init(db::DB::init).auto_remove().unwrap();
                log::info!("scan finished, sleep 1 hour");
            }
        }
    }
}

pub(crate) fn args() -> &'static Args {
    ARGS.get_or_init(Args::parse)
}

async fn run() -> Result<()> {
    let resp = yande::get("https://yande.re/post/popular_recent").await?;

    let mut image_list = yande::get_image_list(&resp)?;

    let resp = yande::get("https://yande.re/post/popular_recent?period=1w")
        .await
        .unwrap_or_default();

    image_list.extend(yande::get_image_list(&resp).unwrap_or_default());
    let download_list = yande::get_download_list(image_list).await?;

    let semaphore = Arc::new(Semaphore::new(args().thread));
    let mut tasks = Vec::new();
    for (id, img_data) in download_list {
        if STOP_SIGNAL.load(Ordering::Relaxed) {
            break;
        }
        let semaphore_clone = Arc::clone(&semaphore);
        tasks.push(tokio::spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();
            let msg =
                format!("来源：[https://yande.re/post/show/{id}](https://yande.re/post/show/{id})");
            bot::send_msg(&msg)
                .await
                .unwrap_or_else(|e| log::error!("send msg failed: {}", e));
            for (id, url) in img_data.url.iter() {
                log::info!("prepare download: {}", id);
                let path = match yande::download_img((*id, url)).await {
                    Ok(path) => path,
                    Err(e) => {
                        log::error!("download failed: {}", e);
                        continue;
                    }
                };

                let path = match resize::resize_and_compress(&path) {
                    Ok(path) => path,
                    Err(e) => {
                        log::error!("resize {id} failed: {}", e);
                        continue;
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
        task.await?;
    }
    Ok(())
}
