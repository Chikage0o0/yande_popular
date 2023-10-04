use anyhow::Result;

use reqwest::header::HeaderMap;
use reqwest::multipart::{self, Form};
use reqwest::{header, Client, ClientBuilder, Method};

use std::path::Path;
use std::sync::OnceLock;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::args;

static CLIENT: OnceLock<Client> = OnceLock::new();

const CHUNK_SIZE: usize = 200 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct PrepareUpload {
    pub content_type: String,
    pub filename: String,
}

#[derive(Debug, serde::Serialize)]
struct UploadChunk {
    file_id: String,
    chunk_data: Vec<u8>,
    chunk_is_last: bool,
}

impl From<UploadChunk> for Form {
    fn from(val: UploadChunk) -> Self {
        let mut form = Form::new();
        form = form.text("file_id", val.file_id);
        form = form.text("chunk_is_last", val.chunk_is_last.to_string());
        form = form.part("chunk_data", multipart::Part::bytes(val.chunk_data));
        form
    }
}
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct UploadResponse {
    pub path: String,
    size: i64,
    hash: String,
    image_properties: Option<ImageProperties>,
}
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct ImageProperties {
    width: i64,
    height: i64,
}

fn client_builder() -> Client {
    let mut headers = header::HeaderMap::new();

    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36 Edg/117.0.2045.47"),
    );

    headers.insert(
        header::HeaderName::from_static("x-api-key"),
        header::HeaderValue::from_str(&args().api_key).unwrap(),
    );

    ClientBuilder::new()
        .default_headers(headers)
        .build()
        .unwrap()
}

async fn prepare_upload(fileinfo: PrepareUpload) -> Result<String> {
    let url = format!("{}/api/bot/file/prepare", &args().server_domain);
    let client = CLIENT.get_or_init(client_builder);
    let resp = client
        .post(&url)
        .json(&fileinfo)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let resp: String = serde_json::from_str(&resp)?;
    Ok(resp)
}

async fn upload(file_path: &Path, file_id: &str) -> Result<UploadResponse> {
    let url = format!("{}/api/bot/file/upload", &args().server_domain);

    let client = CLIENT.get_or_init(client_builder);
    let mut headers = header::HeaderMap::new();
    // headers.insert(
    //     header::CONTENT_TYPE,
    //     header::HeaderValue::from_str("multipart/form-data").unwrap(),
    // );
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_str("application/json; charset=utf-8").unwrap(),
    );

    let mut file = File::open(file_path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len() as usize;
    let mut offset = 0usize;

    while offset < file_size {
        let mut buffer = vec![0; CHUNK_SIZE];
        let chunk_size = file.read(&mut buffer).await?;
        buffer.truncate(chunk_size); // Resize the buffer to the actual number of bytes read
        let is_last_chunk = offset + chunk_size >= file_size;
        let chunk = UploadChunk {
            file_id: file_id.to_string(),
            chunk_data: buffer,
            chunk_is_last: is_last_chunk,
        };

        let resp = client
            .request(Method::POST, &url)
            .headers(headers.clone())
            .multipart(chunk.into())
            .build()?;
        let resp = client.execute(resp).await?.error_for_status()?;

        offset += chunk_size;
        if is_last_chunk {
            return Ok(resp.json().await?);
        }
    }

    Err(anyhow::anyhow!("upload failed"))
}

async fn send_msg(channel_id: &str, msg: &str, header: HeaderMap) -> Result<()> {
    let url = format!(
        "{}/api/bot/send_to_group/{}",
        &args().server_domain,
        channel_id
    );
    let client = CLIENT.get_or_init(client_builder);

    let resp = client
        .request(Method::POST, &url)
        .body(msg.to_string())
        .headers(header)
        .build()?;
    client.execute(resp).await?.error_for_status()?;

    Ok(())
}

pub async fn send_attachment(file_path: &Path) -> Result<()> {
    let mime = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();
    let fileinfo = PrepareUpload {
        content_type: mime,
        filename: file_path.file_name().unwrap().to_str().unwrap().to_string(),
    };
    let file_id = prepare_upload(fileinfo).await?;
    let upload_path = upload(file_path, &file_id).await?.path;

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
    send_msg(channel_id, &msg, headers).await?;
    std::fs::remove_file(file_path)?;
    Ok(())
}
