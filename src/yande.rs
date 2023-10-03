use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    path::PathBuf,
    sync::OnceLock,
};

use crate::{args, db::DB};
use anyhow::{Ok, Result};
use reqwest::{header, Client, ClientBuilder};
use select::{
    document::{self, Document},
    predicate::{Attr, Class, Name},
};

pub static CLIENT: OnceLock<Client> = OnceLock::new();
pub static DB_HANDLE: OnceLock<DB> = OnceLock::new();

type ImgInfo = HashMap<i64, ImgData>;
#[derive(Debug, Clone)]
pub struct ImgData {
    pub score: u64,
    pub url: VecDeque<(i64, String)>,
}

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

pub fn get_image_list(html: &str) -> Result<Vec<i64>> {
    let document = document::Document::from(html);

    let list = document
        .find(Name("ul"))
        .find(|node| node.attr("id") == Some("post-list-posts"))
        .ok_or(anyhow::anyhow!("not found ul#post-list-posts"))?;

    let mut image_list = Vec::new();

    for node in list.children().filter(|node| node.is(Name("li"))) {
        let id = node.attr("id").ok_or(anyhow::anyhow!("not found id"))?;

        let id = id.trim().replace('p', "").parse::<i64>()?;
        image_list.push(id);
    }

    Ok(image_list)
}

async fn find_parent(id: i64) -> Result<(i64, Document)> {
    let url = format!("https://yande.re/post/show/{}", id);
    let html = get(&url).await?;

    let document = document::Document::from(html.as_str());

    let result = match document.find(Class("status-notice")).find(|node| {
        node.children()
            .any(|node| node.is(Name("a")) && node.text().contains("parent post"))
    }) {
        Some(node) => {
            let id = node
                .find(Name("a"))
                .find(|node| {
                    node.attr("href")
                        .is_some_and(|href| href.starts_with("/post/show/"))
                })
                .ok_or(anyhow::anyhow!("not found parent post"))?
                .attr("href")
                .ok_or(anyhow::anyhow!("not found href"))?
                .trim()
                .replace("/post/show/", "")
                .parse::<i64>()?;
            let url = format!("https://yande.re/post/show/{}", id);
            let html = get(&url).await?;

            (id, document::Document::from(html.as_str()))
        }
        None => (id, document),
    };

    Ok(result)
}

pub async fn get_image_info(id: i64) -> Result<(i64, ImgData)> {
    let (id, document) = find_parent(id).await?;

    let mut download_link: VecDeque<(i64, String)> = VecDeque::new();

    let mut score = find_score(id, &document)?;

    download_link.push_back((id, find_raw_url(&document)?));

    let child = document.find(Class("status-notice")).find(|node| {
        node.children()
            .any(|node| node.is(Name("a")) && node.text().contains("child post"))
    });

    if let Some(node) = child {
        for node in node.children().filter(|node| {
            node.is(Name("a"))
                && node
                    .attr("href")
                    .is_some_and(|href| href.starts_with("/post/show/"))
        }) {
            let id = node.text().trim().parse::<i64>()?;

            let html = get(&format!("https://yande.re/post/show/{}", id)).await?;
            let document = document::Document::from(html.as_str());

            let score_child = find_score(id, &document)?;
            if score_child > score {
                score = score_child;
            }

            download_link.push_back((id, find_raw_url(&document)?));
        }
    }

    Ok((
        id,
        ImgData {
            score,
            url: download_link,
        },
    ))
}

fn find_score(id: i64, document: &Document) -> Result<u64> {
    let score = document
        .find(Attr("id", format!("post-score-{}", id).as_str()))
        .find(|node| node.is(Name("span")))
        .ok_or(anyhow::anyhow!("not found span#post-score-{id}"))?
        .text()
        .trim()
        .parse::<u64>()?;
    Ok(score)
}

fn find_raw_url(document: &Document) -> Result<String> {
    let url = document
        .find(Name("a"))
        .find(|node| node.attr("id") == Some("highres"))
        .ok_or(anyhow::anyhow!("not found a#highres"))?
        .attr("href")
        .ok_or(anyhow::anyhow!("not found href"))?;
    Ok(url.to_string())
}

pub async fn get_download_list(image_list: Vec<i64>) -> Result<ImgInfo> {
    let mut download_list = HashMap::new();

    for img_id in image_list {
        if DB_HANDLE
            .get_or_init(DB::init)
            .contains(&img_id.to_string())?
        {
            continue;
        }

        let (id, img_data) = get_image_info(img_id).await?;
        if img_data.score < 50 || DB_HANDLE.get_or_init(DB::init).contains(&id.to_string())? {
            continue;
        }

        download_list.insert(id, img_data.clone());
        for (id, _) in img_data.url.iter() {
            DB_HANDLE.get_or_init(DB::init).insert(&id.to_string())?;
        }
    }

    Ok(download_list)
}

pub async fn download_img((id, url): (i64, &str)) -> Result<(PathBuf, String)> {
    let client = CLIENT.get_or_init(client_builder);

    let resp = client.get(url).send().await?;
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

    #[tokio::test]
    async fn test_get_image_info() {
        let image_info = get_image_info(1124159).await.unwrap();
        println!("{:#?}", image_info);
        assert!(!image_info.1.url.is_empty());
    }
}
