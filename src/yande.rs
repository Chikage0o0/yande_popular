use std::{collections::HashMap, io::Write, path::PathBuf, sync::OnceLock};

use crate::{args, db::DB};
use anyhow::Result;
use reqwest::{header, Client, ClientBuilder};
use select::{document, predicate::Name};

pub static CLIENT: OnceLock<Client> = OnceLock::new();
pub static DB_HANDLE: OnceLock<DB> = OnceLock::new();

fn client_builder() -> Client {
    let mut headers = header::HeaderMap::new();

    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36 Edg/117.0.2045.47"),
    );

    ClientBuilder::new()
        .default_headers(headers)
        .build()
        .unwrap()
}

pub async fn get(url: &str) -> Result<String> {
    let client = CLIENT.get_or_init(client_builder);
    let resp = client.get(url).send().await?.text().await?;
    Ok(resp)
}

pub fn get_image_list(html: &str) -> Result<HashMap<String, String>> {
    let document = document::Document::from(html);

    let list = document
        .find(Name("ul"))
        .find(|node| node.attr("id") == Some("post-list-posts"))
        .ok_or(anyhow::anyhow!("not found ul#post-list-posts"))?;

    let mut image_list = HashMap::new();

    for node in list.children().filter(|node| node.is(Name("li"))) {
        let id = node.attr("id").ok_or(anyhow::anyhow!("not found id"))?;
        let url = node
            .find(Name("a"))
            .find(|node| node.attr("class") == Some("directlink largeimg"))
            .ok_or(anyhow::anyhow!("not found a.directlink largeimg"))?
            .attr("href")
            .ok_or(anyhow::anyhow!("not found href"))?;

        image_list.insert(id.to_string(), url.to_string());
    }

    Ok(image_list)
}

pub fn get_download_list(image_list: &HashMap<String, String>) -> Result<HashMap<String, String>> {
    let mut download_list = HashMap::new();

    for (id, url) in image_list {
        if DB_HANDLE.get_or_init(DB::init).contains(id)? {
            continue;
        }

        download_list.insert(id.to_string(), url.to_string());
        DB_HANDLE.get_or_init(DB::init).insert(id)?;
    }

    Ok(download_list)
}

pub async fn download_img((id, url): (String, String)) -> Result<(PathBuf, String)> {
    let client = CLIENT.get_or_init(client_builder);

    let resp = client.get(&url).send().await?;
    let mime = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .ok_or(anyhow::anyhow!("not found content-type"))?
        .to_str()?
        .to_string();
    let ext = url.split('.').last().unwrap();
    let bytes = resp.bytes().await?;

    let path = PathBuf::from(format!("{}/tmp/{}.{}", &args().data_dir, id, ext));

    let mut file = std::fs::File::create(&path)?;
    file.write_all(&bytes)?;

    Ok((path, mime))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_get() {
        let resp = get("https://yande.re/post/popular_recent").await.unwrap();
        assert!(resp.contains("yande.re"));
    }

    #[tokio::test]
    async fn test_get_image_list() {
        let resp = get("https://yande.re/post/popular_recent").await.unwrap();
        let image_list = get_image_list(&resp).unwrap();
        println!("{:#?}", image_list);
        assert!(!image_list.is_empty());
    }
}
