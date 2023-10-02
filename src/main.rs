use std::sync::{Arc, OnceLock};

use reqwest::header;
use tokio::sync::Semaphore;

use clap::Parser;
use yande::DB_HANDLE;

mod bot;
mod db;
mod yande;

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
    #[arg(short, long, default_value = "data")]
    data_dir: String,

    /// 并发线程数
    /// 默认为4
    #[arg(short, long, default_value = "4")]
    thread: usize,
}

static ARGS: OnceLock<Args> = OnceLock::new();

#[tokio::main]
async fn main() {
    env_logger::init();
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

    let image_list = yande::get_image_list(&resp).unwrap();
    let download_list = yande::get_download_list(&image_list).unwrap();

    let semaphore = Arc::new(Semaphore::new(args().thread));
    let mut tasks = Vec::new();
    for (id, url) in download_list {
        let semaphore_clone = Arc::clone(&semaphore);
        tasks.push(tokio::spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();

            let (path, mime) = match yande::download_img((id, url)).await {
                Ok(path) => path,
                Err(e) => {
                    log::error!("download failed: {}", e);
                    return;
                }
            };

            let fileinfo = bot::PrepareUpload {
                content_type: mime,
                filename: path.file_name().unwrap().to_str().unwrap().to_string(),
            };

            let resp = match bot::prepare_upload(fileinfo).await {
                Ok(resp) => resp,
                Err(e) => {
                    log::error!("prepare_upload failed: {}", e);
                    return;
                }
            };

            let file_id = resp;

            let upload_path = match bot::upload(&path, &file_id).await {
                Ok(resp) => resp.path,
                Err(e) => {
                    log::error!("upload failed: {}", e);

                    return;
                }
            };

            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("vocechat/file"),
            );
            let channel_id = &args().channel_id;
            let payload = serde_json::json!({
                "path":upload_path,
            });
            let msg = serde_json::to_string(&payload).unwrap();
            match bot::send_msg(channel_id, &msg, headers).await {
                Ok(_) => {
                    std::fs::remove_file(&path).unwrap();
                }
                Err(e) => {
                    log::error!("send_msg failed: {}", e);
                }
            };
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }
}
